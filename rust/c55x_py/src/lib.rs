//! The core as the Python extension module c55x_core (imported by c55x_emu/rust_core.py
//! from build/py): the Core class over the Python host, its containment checks and the
//! conversion of errors to Python exceptions.

mod frontend;
mod pyhost;
mod sync;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use c55x_core::core::Core;
use c55x_core::err::Error;
use c55x_core::icache::{PcFilter, PcSet};

use crate::pyhost::{peek_event, PyHost};

pyo3::create_exception!(
    c55x_core, ResourcePolicyError, pyo3::exceptions::PyRuntimeError,
    "Raised when the core is used outside the managed resource containment."
);

/// Full containment check (constructor, and the first entry after a failure).
fn guard() -> PyResult<()> {
    c55x_core::guard::guard().map_err(ResourcePolicyError::new_err)
}

/// Containment check at an execution entry of an existing core (revalidated every
/// c55x_core::guard::REVALIDATE_EVERY).
fn guard_entry() -> PyResult<()> {
    c55x_core::guard::guard_entry().map_err(ResourcePolicyError::new_err)
}

/// Most pcs a trace may hold (4 Mi entries = 16 MiB): a resource cap, not
/// architectural state.
const MAX_TRACE_LEN: usize = 1 << 22;

fn to_py(py: Python<'_>, e: Error) -> PyErr {
    match e {
        Error::Host(e) => match e.downcast::<PyErr>() {
            Ok(e) => *e,
            Err(_) => pyo3::exceptions::PyRuntimeError::new_err("host error"),
        },
        Error::Emu(msg) | Error::Unsupported(msg) => match py.import("c55x_emu.registers") {
            Ok(m) => match m.getattr("EmulationError").and_then(|c| c.call1((msg,))) {
                Ok(inst) => PyErr::from_value(inst),
                Err(e) => e,
            },
            Err(e) => e,
        },
    }
}

#[pyclass(unsendable, name = "Core")]
struct PyCore {
    core: Core,
}

impl PyCore {
    fn pyhost(&self) -> &PyHost {
        self.core.host.as_any().downcast_ref().expect("a PyCore always has a PyHost")
    }

    fn pyhost_mut(&mut self) -> &mut PyHost {
        self.core.host.as_any_mut().downcast_mut().expect("a PyCore always has a PyHost")
    }

    /// Load the state from the Python Cpu (it is authoritative between calls).
    fn enter<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        // Re-validate containment at execution entries (guard.rs REVALIDATE_EVERY):
        // detects a process moved out of the cgroup or limits relaxed after
        // construction.
        guard_entry()?;
        let cpu = self.pyhost().cpu.bind(py).clone();
        let hooks = self.pyhost().data_hooks(py)?;
        let c = &mut self.core;
        sync::load(c, py, &cpu)?;
        c.set_hooks(hooks);
        c.next_event = peek_event(py, &cpu)?;
        let idle = cpu.getattr("bios_idle")?;
        c.bios_idle = if idle.is_none() { None } else { Some(idle.extract()?) };
        c.poll_loops = PcFilter::new(cpu.getattr("poll_loops")?.extract()?);
        Ok(cpu)
    }

    /// Store the state back (also after an error, so the Python side can report it).
    fn leave<T>(&mut self, py: Python<'_>, cpu: &Bound<'_, PyAny>, r: Result<T, Error>) -> PyResult<T> {
        sync::store(&mut self.core, py, cpu)?;
        r.map_err(|e| to_py(py, e))
    }
}

#[pymethods]
impl PyCore {
    #[new]
    fn new(py: Python<'_>, cpu: &Bound<'_, PyAny>) -> PyResult<Self> {
        // The core refuses to be built outside the managed cgroup.
        guard()?;
        let (host, mem, code_pages) = PyHost::new(py, cpu)?;
        Ok(PyCore { core: Core::new(Box::new(host), mem, code_pages) })
    }

    fn __traverse__(&self, visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        match self.core.host.as_any().downcast_ref::<PyHost>() {
            Some(h) => h.visit(&visit),
            None => Ok(()),
        }
    }

    /// Cpu.run: until idle (-> True), max_steps, a pc in stop or the cycle limit until.
    #[pyo3(signature = (max_steps, stop, until=None))]
    fn run(&mut self, py: Python<'_>, max_steps: u64, stop: PcSet, until: Option<u64>) -> PyResult<bool> {
        let cpu = self.enter(py)?;
        let r = self.core.run(max_steps, &PcFilter::new(stop), until);
        self.leave(py, &cpu, r)
    }

    /// Cpu.step: one instruction (an enabled pending interrupt is taken first).
    fn step(&mut self, py: Python<'_>) -> PyResult<()> {
        let cpu = self.enter(py)?;
        let r = self.core.step();
        self.leave(py, &cpu, r)
    }

    /// Cpu.call: push the return context like a call instruction; the pc is set to target.
    fn call(&mut self, py: Python<'_>, target: u32, ret: u32) -> PyResult<()> {
        let cpu = self.enter(py)?;
        let r = self.core.call(target, ret).map(|t| self.core.st.pc = t);
        self.leave(py, &cpu, r)
    }

    /// Return from the current function like a return instruction (the pc is set), for
    /// functions replaced by Python code (machine.py hle).
    fn ret(&mut self, py: Python<'_>) -> PyResult<()> {
        let cpu = self.enter(py)?;
        let r = self.core.ret().map(|t| self.core.st.pc = t);
        self.leave(py, &cpu, r)
    }

    /// Attach the ATA disk (ata.py): disk.Disk-like object, its size, IDENTIFY words.
    fn ata_attach(&mut self, disk: Py<PyAny>, sectors: u64, identify: Vec<u16>) -> PyResult<()> {
        if identify.len() != 256 { return Err(pyo3::exceptions::PyValueError::new_err("IDENTIFY: 256 words")); }
        self.pyhost_mut().disk = Some(disk);
        self.core.ata = Some(Box::new(c55x_core::ata::Ata::new(sectors, identify)));
        Ok(())
    }

    /// An ATA register access from Python (tools, tests), as the CPU would make it.
    #[pyo3(signature = (word, value=None))]
    fn ata_io(&mut self, py: Python<'_>, word: u32, value: Option<u32>) -> PyResult<u32> {
        if !self.core.is_ata(word) { return Err(pyo3::exceptions::PyValueError::new_err("not an ATA register")); }
        let cpu = self.enter(py)?;
        let r = match value { None => self.core.ata_read(word), Some(v) => self.core.ata_write(word, v).map(|_| 0) };
        self.leave(py, &cpu, r)
    }

    /// ATA counters: ({command: count}, {unmodeled access: count}).
    fn ata_stats(&self) -> PyResult<(std::collections::BTreeMap<u8, u64>, std::collections::BTreeMap<String, u64>)> {
        let a = self.core.ata.as_ref().ok_or_else(|| pyo3::exceptions::PyValueError::new_err("no ATA attached"))?;
        Ok((a.commands.clone(), a.unknown.clone()))
    }

    /// Let cycles pass without executing instructions: due core timers and Python events fire.
    fn advance(&mut self, py: Python<'_>, cycles: u64) -> PyResult<()> {
        let cpu = self.enter(py)?;
        let r = self.core.advance(cycles);
        self.leave(py, &cpu, r)
    }

    /// DMA channel n registers (0x0c00 + 0x20 n .. + 15), for debugging.
    fn dma_regs(&self, n: usize) -> PyResult<Vec<u16>> {
        if n >= 6 { return Err(pyo3::exceptions::PyValueError::new_err("DMA channel 0..5")); }
        Ok(self.core.dma_regs(n))
    }

    /// Record the last max_len executed pcs (0 switches tracing off).
    fn set_trace(&mut self, max_len: usize) -> PyResult<()> {
        if max_len > MAX_TRACE_LEN {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "trace length {} exceeds the maximum {}", max_len, MAX_TRACE_LEN)));
        }
        self.core.trace = if max_len == 0 { None } else { Some((max_len, Default::default())) };
        Ok(())
    }

    /// The recorded pcs, oldest first.
    fn trace(&self) -> Vec<u32> {
        self.core.trace.as_ref().map(|(_, t)| t.iter().copied().collect()).unwrap_or_default()
    }

    /// The four accumulator overflow flags ACOV0..ACOV3, 0 (clear) or 1 (set) each.
    fn acov(&self) -> Vec<u32> {
        self.core.st.acov.to_vec()
    }

    /// Count long accesses at odd addresses per pc (on: start afresh, off: stop).
    fn set_odd_longs(&mut self, on: bool) {
        self.core.odd_longs = if on { Some(Default::default()) } else { None };
    }

    /// {pc: count} of the long accesses at odd addresses since set_odd_longs(True).
    fn odd_longs(&self) -> std::collections::BTreeMap<u32, u64> {
        self.core.odd_longs.clone().unwrap_or_default()
    }

    /// Counters: executed instructions, compiled instructions, Python bus writes and calls.
    fn stats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        d.set_item("native", self.core.native)?;
        d.set_item("compiles", self.core.compiles)?;
        d.set_item("py_writes", self.core.py_writes)?;
        d.set_item("py_calls", self.core.py_calls)?;
        d.set_item("dma_blocks", self.core.dma.blocks.to_vec())?;
        d.set_item("mcbsp_underruns", self.core.mcbsp.iter().map(|m| m.underruns).collect::<Vec<_>>())?;
        Ok(d)
    }

    /// Audio McBSP0 sent since the last call: [(sample rate, little-endian int16 L/R bytes)].
    fn take_audio<'py>(&mut self, py: Python<'py>) -> Vec<(f64, Bound<'py, PyBytes>)> {
        self.core.audio_samples = 0;
        std::mem::take(&mut self.core.audio).into_iter()
            .map(|(rate, s)| {
                let raw: Vec<u8> = s.iter().flat_map(|v| v.to_le_bytes()).collect();
                (rate, PyBytes::new(py, &raw))
            })
            .collect()
    }
}

#[pymodule]
#[pyo3(name = "c55x_core")]
fn c55x_core_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyCore>()?;
    m.add("ResourcePolicyError", m.py().get_type::<ResourcePolicyError>())?;
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;
    frontend::register(m)?;
    Ok(())
}
