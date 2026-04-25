use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    InvalidFormat(String),
    Unsupported(String),
    FieldNotFound(String),
    InvalidFieldSpec(String),
    Overflow(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidFormat(message) => write!(f, "{message}"),
            Self::Unsupported(message) => write!(f, "{message}"),
            Self::FieldNotFound(name) => write!(f, "field not found: {name}"),
            Self::InvalidFieldSpec(message) => write!(f, "{message}"),
            Self::Overflow(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}
