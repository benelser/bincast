use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    TomlParse(TomlError),
    Config(String),
    Validation(Vec<String>),
    Cli(String),
}

#[derive(Debug)]
pub struct TomlError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {e}"),
            Error::TomlParse(e) => write!(f, "TOML parse error at {}:{}: {}", e.line, e.col, e.message),
            Error::Config(msg) => write!(f, "config error: {msg}"),
            Error::Validation(errors) => {
                writeln!(f, "validation errors:")?;
                for e in errors {
                    writeln!(f, "  - {e}")?;
                }
                Ok(())
            }
            Error::Cli(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<TomlError> for Error {
    fn from(e: TomlError) -> Self {
        Error::TomlParse(e)
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Config(s)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
