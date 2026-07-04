//! The crate-wide error type: a plain message, with `From` impls for the
//! library errors that cross into it (TOML parsing, IO, postcard).

#[derive(Debug)]
pub struct Error {
    pub error_message: String,
}

impl Error {
    pub fn new(error_message: impl Into<String>) -> Error {
        Error {
            error_message: error_message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_message)
    }
}

impl std::error::Error for Error {}

impl From<toml::de::Error> for Error {
    fn from(error: toml::de::Error) -> Error {
        Error {
            error_message: error.to_string(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error {
            error_message: error.to_string(),
        }
    }
}

impl From<postcard::Error> for Error {
    fn from(error: postcard::Error) -> Error {
        Error {
            error_message: error.to_string(),
        }
    }
}
