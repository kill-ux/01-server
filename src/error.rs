use core::fmt;
use std::{
    error::Error,
    fmt::{Debug, Display, Formatter},
};

use parser::YamlError;

pub struct CleanError(pub Box<dyn Error>);

impl Debug for CleanError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "\r\x1b[K{}", self)
    }
}

impl Display for CleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\x1b[31mERROR\x1b[0m: {}", self.0)
    }
}

impl Error for CleanError {}

impl From<YamlError> for CleanError {
    fn from(e: YamlError) -> Self {
        CleanError(Box::new(e))
    }
}

impl From<std::io::Error> for CleanError {
    fn from(e: std::io::Error) -> Self {
        CleanError(Box::new(e))
    }
}

impl From<std::net::AddrParseError> for CleanError {
    fn from(e: std::net::AddrParseError) -> Self {
        CleanError(Box::new(e))
    }
}

impl From<String> for CleanError {
    fn from(s: String) -> Self {
        // Use a simple custom error type to wrap the string
        CleanError(Box::new(YamlError::Generic(s)))
    }
}

impl From<&str> for CleanError {
    fn from(s: &str) -> Self {
        CleanError(Box::new(YamlError::Generic(s.to_string())))
    }
}

pub type Result<T> = std::result::Result<T, CleanError>;
