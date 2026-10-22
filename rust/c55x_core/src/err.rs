//! Errors: emulation errors, unsupported instruction forms (an instruction the core
//! cannot compile; executing it is an emulation error) and containment refusals.

pub enum Error {
    Emu(String),
    Unsupported(String),
    Policy(String),                     // outside the managed containment (guard.rs)
}

pub type R<T> = Result<T, Error>;

pub fn emu<T>(msg: String) -> R<T> {
    Err(Error::Emu(msg))
}

pub fn unsupported<T>(msg: impl Into<String>) -> R<T> {
    Err(Error::Unsupported(msg.into()))
}
