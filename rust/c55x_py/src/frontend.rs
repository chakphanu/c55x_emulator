//! Python access to the Rust front end (c55x_core::frontend), for the IR equality checks
//! against ir.py (tools/frontend_ir_check.py). Results are (True, value) or (False, error text).

use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyList, PyString, PyTuple};

use c55x_core::ast::Ast;
use c55x_core::err::Error;
use c55x_core::frontend;

fn to_py<'py>(py: Python<'py>, a: &Ast) -> PyResult<Bound<'py, PyAny>> {
    let all = |v: &[Ast]| v.iter().map(|x| to_py(py, x)).collect::<PyResult<Vec<_>>>();
    Ok(match a {
        Ast::None => py.None().into_bound(py),
        Ast::Bool(b) => PyBool::new(py, *b).to_owned().into_any(),
        Ast::Int(v) => v.into_pyobject(py)?.into_any(),
        Ast::Str(s) => PyString::new(py, s).into_any(),
        Ast::List(v) => PyList::new(py, all(v)?)?.into_any(),
        Ast::Tuple(v) => PyTuple::new(py, all(v)?)?.into_any(),
    })
}

fn failure<'py>(py: Python<'py>, e: Error) -> PyResult<(bool, Bound<'py, PyAny>)> {
    match e {
        Error::Unsupported(m) | Error::Emu(m) => Ok((false, PyString::new(py, &m).into_any())),
        Error::Host(_) | Error::Policy(_) => Err(pyo3::exceptions::PyRuntimeError::new_err("host error")),
    }
}

/// IR of one instruction: ir.build(Insn(addr, size, text, block_end), resolve) with
/// Cpu.resolve over symbols.
#[pyfunction]
#[pyo3(signature = (text, addr, size, block_end=None, symbols=None))]
fn frontend_ir<'py>(
    py: Python<'py>, text: &str, addr: i64, size: i64, block_end: Option<i64>,
    symbols: Option<HashMap<String, i64>>,
) -> PyResult<(bool, Bound<'py, PyAny>)> {
    let symbols = symbols.unwrap_or_default();
    let resolve = frontend::resolver(&symbols);
    match frontend::build_text(text, addr, size, block_end, &resolve) {
        Ok(ir) => Ok((true, to_py(py, &ir)?)),
        Err(e) => failure(py, e),
    }
}

/// listing.parse(text, markers) as [(addr, size, text, block_end)].
#[pyfunction]
#[pyo3(signature = (text, markers=true))]
fn frontend_listing<'py>(py: Python<'py>, text: &str, markers: bool) -> PyResult<(bool, Bound<'py, PyAny>)> {
    match frontend::listing::parse(text, markers) {
        Ok(insns) => {
            let rows: Vec<(u32, u32, String, Option<u32>)> = insns.into_values()
                .map(|i| (i.addr, i.size, i.text, i.block_end)).collect();
            Ok((true, rows.into_pyobject(py)?.into_any()))
        }
        Err(e) => failure(py, e),
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(frontend_ir, m)?)?;
    m.add_function(wrap_pyfunction!(frontend_listing, m)?)?;
    Ok(())
}
