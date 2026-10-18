//! Errors: emulation errors become c55x_emu.registers.EmulationError at the Python
//! boundary, unsupported instruction forms make the core fall back to Python.

use pyo3::PyErr;

pub enum Error {
    Emu(String),
    Unsupported(String),
    Py(PyErr),
}

pub type R<T> = Result<T, Error>;

pub fn emu<T>(msg: String) -> R<T> {
    Err(Error::Emu(msg))
}

pub fn unsupported<T>(msg: impl Into<String>) -> R<T> {
    Err(Error::Unsupported(msg.into()))
}

impl From<PyErr> for Error {
    fn from(e: PyErr) -> Self {
        Error::Py(e)
    }
}
