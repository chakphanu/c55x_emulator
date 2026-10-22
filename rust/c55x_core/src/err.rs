//! Errors: emulation errors become c55x_emu.registers.EmulationError at the Python
//! boundary, unsupported instruction forms make the core fall back to Python, and a host
//! error (for the Python host: the exception raised in a callback) is carried through
//! unchanged.

use std::any::Any;

pub enum Error {
    Emu(String),
    Unsupported(String),
    Policy(String),                     // outside the managed containment (guard.rs)
    Host(Box<dyn Any + Send>),
}

pub type R<T> = Result<T, Error>;

pub fn emu<T>(msg: String) -> R<T> {
    Err(Error::Emu(msg))
}

pub fn unsupported<T>(msg: impl Into<String>) -> R<T> {
    Err(Error::Unsupported(msg.into()))
}
