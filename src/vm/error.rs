use crate::types::{LineRange, Type};
use std::fmt::Display;

#[derive(Debug)]
pub struct RuntimeError {
    pub source: ErrorKind,
    pub traceback: Vec<TracebackFrame>,
}

impl std::error::Error for RuntimeError {}

impl Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}\nstack traceback:\n{}",
            self.source,
            self.traceback
                .iter()
                .map(|frame| {
                    match &frame.lines_defined {
                        LineRange::File => "\tin main chunk".to_owned(),
                        LineRange::Lines(range) => {
                            format!("\tin function <{}:{}>", frame.source, range.start)
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("{0}")]
    ExplicitError(String),

    #[error("attempt to {operation} a {ty} value")]
    TypeError { operation: Operation, ty: Type },

    #[error("bad argument #{nth} ({message})")]
    ArgumentError { nth: usize, message: &'static str },

    #[error("bad argument #{nth} ({expected_type} expected, got {got_type})")]
    ArgumentTypeError {
        nth: usize,
        expected_type: Type,
        got_type: Type,
    },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
}

#[derive(Debug, Clone, Copy)]
pub enum Operation {
    Index,
    Call,
    Concatenate,
}

impl Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Index => f.write_str("index"),
            Self::Call => f.write_str("call"),
            Self::Concatenate => f.write_str("concatenate"),
        }
    }
}

#[derive(Debug)]
pub struct TracebackFrame {
    pub source: String,
    pub lines_defined: LineRange,
}
