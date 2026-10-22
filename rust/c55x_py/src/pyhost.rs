//! The Python host: the Cpu / Bus / CodeCache objects of c55x_emu and ir.build. Every
//! callback runs with cycles, pc and IFR synchronized to the Python Cpu and reads back
//! IFR, the next event and the code pages the bus dropped (writes into code).

use std::any::Any;

use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyByteArray, PyBytes, PyInt, PyList, PyString, PyTuple};

use c55x_core::ast::Ast;
use c55x_core::core::MEM_BYTES;
use c55x_core::err::{emu, Error, R};
use c55x_core::host::{Ctx, Decoded, Host};

/// A Python exception raised in a host callback travels in Error::Host (lib.rs to_py
/// takes it out again).
trait Py2R<T> {
    fn h(self) -> R<T>;
}

impl<T> Py2R<T> for PyResult<T> {
    fn h(self) -> R<T> {
        self.map_err(|e| Error::Host(Box::new(e)))
    }
}

/// The ir.py tuple of an instruction as an Ast (on the compile path, cache miss).
pub fn ast_from_py(o: &Bound<'_, PyAny>) -> R<Ast> {
    Ok(if o.is_none() {
        Ast::None
    } else if let Ok(b) = o.cast::<PyBool>() {
        Ast::Bool(b.is_true())
    } else if o.is_instance_of::<PyInt>() {
        Ast::Int(o.extract::<i64>().h()?)
    } else if let Ok(s) = o.cast::<PyString>() {
        Ast::Str(s.to_str().h()?.to_owned())
    } else if let Ok(t) = o.cast::<PyTuple>() {
        Ast::Tuple(t.iter().map(|x| ast_from_py(&x)).collect::<R<_>>()?)
    } else if let Ok(l) = o.cast::<PyList>() {
        Ast::List(l.iter().map(|x| ast_from_py(&x)).collect::<R<_>>()?)
    } else {
        return emu(format!("IR value of type {}", o.get_type().name().h()?));
    })
}

pub struct PyHost {
    pub cpu: Py<PyAny>,
    bus: Py<PyAny>,
    code: Py<PyAny>,
    ir_build: Py<PyAny>,
    _mem_obj: Py<PyByteArray>,
    pub disk: Option<Py<PyAny>>,             // attached by ata.py
}

fn cfg<'py>(o: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    o.getattr(name)
}

impl PyHost {
    /// The host of cpu, the bus memory pointer and the pages holding decoded code.
    pub fn new(py: Python<'_>, cpu: &Bound<'_, PyAny>) -> PyResult<(PyHost, *mut u8, Vec<bool>)> {
        let bus = cfg(cpu, "bus")?;
        let mem_any = cfg(&bus, "mem")?;
        let mem_obj = mem_any.cast::<PyByteArray>()?.clone();
        if mem_obj.len() != MEM_BYTES {
            return Err(pyo3::exceptions::PyValueError::new_err("bus memory size"));
        }
        let mem = unsafe { mem_obj.as_bytes_mut().as_mut_ptr() };
        let code = cfg(cpu, "code")?;
        let mut code_pages = vec![false; MEM_BYTES >> 8];
        for p in cfg(&code, "pages")?.try_iter()? {
            let p: i64 = p?.extract()?;
            if p >= 0 && (p as usize) < code_pages.len() {
                code_pages[p as usize] = true;
            }
        }
        let ir_build = py.import("c55x_emu.ir")?.getattr("build")?;
        let host = PyHost {
            cpu: cpu.clone().unbind(), bus: bus.unbind(), code: code.unbind(), ir_build: ir_build.unbind(),
            _mem_obj: mem_obj.unbind(), disk: None,
        };
        Ok((host, mem, code_pages))
    }

    /// The hooked data ranges of the bus (peripherals register after the Cpu exists).
    pub fn data_hooks(&self, py: Python<'_>) -> PyResult<Vec<(u32, u32)>> {
        let mut hooks = Vec::new();
        for h in self.bus.bind(py).getattr("data_hooks")?.try_iter()? {
            let h = h?;
            hooks.push((h.get_item(0)?.extract::<u32>()?, h.get_item(1)?.extract::<u32>()?));
        }
        Ok(hooks)
    }

    /// Report every Python object this host holds a strong reference to, so the cycle
    /// Cpu -> core -> Cpu can be found and freed by Python's collector. A reference
    /// missing here would keep its whole cycle alive (a leak of about 16 MiB per core).
    pub fn visit(&self, visit: &pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        visit.call(&self.cpu)?;
        visit.call(&self.bus)?;
        visit.call(&self.code)?;
        visit.call(&self.ir_build)?;
        visit.call(&self._mem_obj)?;
        if let Some(d) = &self.disk {
            visit.call(d)?;
        }
        Ok(())
    }

    /// Call into Python with cycles, pc and IFR synchronized; reads back IFR, the next
    /// event and the dropped code pages afterwards.
    fn sync<T>(&self, ctx: &mut Ctx,
               f: impl FnOnce(Python<'_>, &Bound<'_, PyAny>, &Bound<'_, PyAny>) -> PyResult<T>) -> R<T> {
        Python::attach(|py| -> R<T> {
            let cpu = self.cpu.bind(py).clone();
            let bus = self.bus.bind(py).clone();
            cpu.setattr(intern!(py, "cycles"), ctx.cycles).h()?;
            cpu.setattr(intern!(py, "pc"), ctx.pc).h()?;
            cpu.setattr(intern!(py, "ifr0"), ctx.ifr0).h()?;
            cpu.setattr(intern!(py, "ifr1"), ctx.ifr1).h()?;
            let r = f(py, &cpu, &bus);
            ctx.ifr0 = cpu.getattr(intern!(py, "ifr0")).and_then(|v| v.extract()).h()?;
            ctx.ifr1 = cpu.getattr(intern!(py, "ifr1")).and_then(|v| v.extract()).h()?;
            ctx.next_event = peek_event(py, &cpu).h()?;
            let dropped = self.code.bind(py).getattr(intern!(py, "dropped")).h()?;
            let dropped = dropped.cast::<PyList>().map_err(PyErr::from).h()?;
            if !dropped.is_empty() {
                for p in dropped.iter() {
                    ctx.dropped.push(p.extract().h()?);
                }
                dropped.call_method0(intern!(py, "clear")).h()?;
            }
            r.h()
        })
    }
}

/// The cycle of the first pending Python event (u64::MAX: none).
pub fn peek_event(py: Python<'_>, cpu: &Bound<'_, PyAny>) -> PyResult<u64> {
    let events = cpu.getattr(intern!(py, "events"))?;
    if events.len()? == 0 {
        return Ok(u64::MAX);
    }
    events.get_item(0)?.get_item(0)?.extract()
}

impl Host for PyHost {
    fn read_hooked(&mut self, ctx: &mut Ctx, word: u32) -> R<u32> {
        self.sync(ctx, |py, _cpu, bus| bus.call_method1(intern!(py, "read"), (word,))?.extract())
    }

    fn write_hooked(&mut self, ctx: &mut Ctx, word: u32, v: u32) -> R<()> {
        self.sync(ctx, |py, _cpu, bus| { bus.call_method1(intern!(py, "write"), (word, v))?; Ok(()) })
    }

    fn port_read(&mut self, ctx: &mut Ctx, port: u32) -> R<u32> {
        self.sync(ctx, |py, _cpu, bus| bus.call_method1(intern!(py, "port_read"), (port,))?.extract())
    }

    fn port_write(&mut self, ctx: &mut Ctx, port: u32, v: u32) -> R<()> {
        self.sync(ctx, |py, _cpu, bus| { bus.call_method1(intern!(py, "port_write"), (port, v))?; Ok(()) })
    }

    fn fire_events(&mut self, ctx: &mut Ctx) -> R<()> {
        self.sync(ctx, |py, cpu, _bus| { cpu.call_method0(intern!(py, "_fire_events"))?; Ok(()) })
    }

    fn clock_stopped(&mut self, ctx: &mut Ctx) -> R<bool> {
        self.sync(ctx, |py, cpu, _bus| cpu.getattr(intern!(py, "idle_ctl"))?.call_method0("clock_stopped")?.extract())
    }

    fn idle_execute(&mut self, ctx: &mut Ctx) -> R<Option<bool>> {
        self.sync(ctx, |py, cpu, _bus| {
            let ctl = cpu.getattr(intern!(py, "idle_ctl"))?;
            if ctl.is_none() { return Ok(None); }
            Ok(Some(ctl.call_method0("execute")?.extract::<bool>()?))
        })
    }

    fn audio_clock(&mut self, ctx: &mut Ctx) -> R<Option<(f64, f64)>> {
        self.sync(ctx, |py, cpu, _bus| {
            let f = cpu.getattr(intern!(py, "audio_clock"))?;
            if f.is_none() { return Ok(None); }
            Ok(Some(f.call0()?.extract::<(f64, f64)>()?))
        })
    }

    fn disk_read(&mut self, ctx: &mut Ctx, lba: u64) -> R<Vec<u8>> {
        let Some(disk) = Python::attach(|py| self.disk.as_ref().map(|d| d.clone_ref(py))) else {
            return emu("ATA access without an attached disk".into());
        };
        self.sync(ctx, |py, _cpu, _bus| disk.bind(py).call_method1(intern!(py, "read"), (lba,))?.extract::<Vec<u8>>())
    }

    fn disk_write(&mut self, ctx: &mut Ctx, lba: u64, data: &[u8]) -> R<()> {
        let Some(disk) = Python::attach(|py| self.disk.as_ref().map(|d| d.clone_ref(py))) else {
            return emu("ATA access without an attached disk".into());
        };
        self.sync(ctx, |py, _cpu, _bus| {
            disk.bind(py).call_method1(intern!(py, "write"), (lba, PyBytes::new(py, data)))?;
            Ok(())
        })
    }

    fn decode(&mut self, pc: u32) -> R<Option<Decoded>> {
        Python::attach(|py| -> R<Option<Decoded>> {
            let insn = self.code.bind(py).call_method1(intern!(py, "get"), (pc,)).h()?;
            if insn.is_none() {
                return Ok(None);
            }
            let size: u32 = insn.getattr(intern!(py, "size")).and_then(|v| v.extract()).h()?;
            let text: String = insn.getattr(intern!(py, "text")).and_then(|v| v.extract()).h()?;
            let resolve = self.cpu.bind(py).getattr(intern!(py, "resolve")).h()?;
            let ir = match self.ir_build.bind(py).call1((&insn, resolve)) {
                Err(e) => Err(e.to_string()),
                Ok(ir) => Ok(ast_from_py(&ir)?),
            };
            Ok(Some(Decoded { size, text, ir }))
        })
    }

    fn code_pages_around(&mut self, first: u32, last: u32, out: &mut Vec<(u32, bool)>) -> R<()> {
        Python::attach(|py| -> R<()> {
            let pages = self.code.bind(py).getattr(intern!(py, "pages")).h()?;
            for p in first..=last {
                out.push((p, pages.contains(p).h()?));
            }
            Ok(())
        })
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
