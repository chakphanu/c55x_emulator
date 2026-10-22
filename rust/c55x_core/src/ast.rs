//! The instruction IR of ir.py as plain data. compile.rs and stmt.rs read an Ast instead
//! of Python objects; the Python tuple is converted once, on the compile (cache miss)
//! path. Access follows the Python rules the compiler relied on: a bool is an int, an int
//! is not a bool, str / list / tuple are distinct, == compares like Python.

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyInt, PyList, PyString, PyTuple};

use crate::err::{emu, R};

pub enum Ast {
    None,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<Ast>),
    Tuple(Vec<Ast>),
}

impl Ast {
    pub fn from_py(o: &Bound<'_, PyAny>) -> R<Ast> {
        Ok(if o.is_none() {
            Ast::None
        } else if let Ok(b) = o.cast::<PyBool>() {
            Ast::Bool(b.is_true())
        } else if o.is_instance_of::<PyInt>() {
            Ast::Int(o.extract::<i64>()?)
        } else if let Ok(s) = o.cast::<PyString>() {
            Ast::Str(s.to_str()?.to_owned())
        } else if let Ok(t) = o.cast::<PyTuple>() {
            Ast::Tuple(t.iter().map(|x| Ast::from_py(&x)).collect::<R<_>>()?)
        } else if let Ok(l) = o.cast::<PyList>() {
            Ast::List(l.iter().map(|x| Ast::from_py(&x)).collect::<R<_>>()?)
        } else {
            return emu(format!("IR value of type {}", o.get_type().name()?));
        })
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Ast::None)
    }

    fn items(&self) -> R<&[Ast]> {
        match self {
            Ast::List(v) | Ast::Tuple(v) => Ok(v),
            _ => emu("IR value is not a sequence".into()),
        }
    }

    pub fn get_item(&self, i: usize) -> R<&Ast> {
        match self.items()?.get(i) {
            Some(x) => Ok(x),
            None => emu(format!("IR index {} out of range", i)),
        }
    }

    pub fn len(&self) -> R<usize> {
        Ok(self.items()?.len())
    }

    pub fn try_iter(&self) -> R<std::slice::Iter<'_, Ast>> {
        Ok(self.items()?.iter())
    }

    pub fn extract<T: FromAst>(&self) -> R<T> {
        T::from_ast(self)
    }

    /// Python ==.
    pub fn eq(&self, other: &Ast) -> R<bool> {
        Ok(match (self, other) {
            (Ast::None, Ast::None) => true,
            (Ast::Str(a), Ast::Str(b)) => a == b,
            (Ast::List(a), Ast::List(b)) | (Ast::Tuple(a), Ast::Tuple(b)) => {
                if a.len() != b.len() { return Ok(false); }
                for (x, y) in a.iter().zip(b) {
                    if !x.eq(y)? { return Ok(false); }
                }
                true
            }
            (a, b) => match (a.int(), b.int()) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            },
        })
    }

    fn int(&self) -> Option<i64> {
        match self {
            Ast::Int(v) => Some(*v),
            Ast::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }
}

pub trait FromAst: Sized {
    fn from_ast(a: &Ast) -> R<Self>;
}

impl FromAst for bool {
    fn from_ast(a: &Ast) -> R<bool> {
        match a { Ast::Bool(b) => Ok(*b), _ => emu("IR value is not a bool".into()) }
    }
}

impl FromAst for i64 {
    fn from_ast(a: &Ast) -> R<i64> {
        match a.int() { Some(v) => Ok(v), None => emu("IR value is not an int".into()) }
    }
}

impl FromAst for u8 {
    fn from_ast(a: &Ast) -> R<u8> {
        match a.int() {
            Some(v) if (0..=255).contains(&v) => Ok(v as u8),
            _ => emu("IR value is not a u8".into()),
        }
    }
}

impl FromAst for String {
    fn from_ast(a: &Ast) -> R<String> {
        match a { Ast::Str(s) => Ok(s.clone()), _ => emu("IR value is not a str".into()) }
    }
}

impl FromAst for Option<i64> {
    fn from_ast(a: &Ast) -> R<Option<i64>> {
        if a.is_none() { Ok(None) } else { Ok(Some(i64::from_ast(a)?)) }
    }
}

impl FromAst for Vec<usize> {
    fn from_ast(a: &Ast) -> R<Vec<usize>> {
        a.items()?.iter().map(|x| match x.int() {
            Some(v) if v >= 0 => Ok(v as usize),
            _ => emu("IR value is not an index".into()),
        }).collect()
    }
}
