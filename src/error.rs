use std::io::Error as IoError;

use reqwest::Error as ReqwestError;

#[derive(Debug)]
pub enum Error {
    Io(IoError),
    Reqwest(ReqwestError),
    Other(String),
}

impl Error {
    pub fn other<S>(msg: S) -> Self
    where
        S: Into<String>,
    {
        Self::Other(msg.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match &self {
            Self::Io(e) => format!("[Io]: {e}"),
            Self::Reqwest(e) => format!("[Reqwest]: {e}"),
            Self::Other(e) => e.to_owned(),
        };

        write!(f, "{}", msg)
    }
}

impl From<IoError> for Error {
    fn from(value: IoError) -> Self {
        Self::Io(value)
    }
}

impl From<ReqwestError> for Error {
    fn from(value: ReqwestError) -> Self {
        Self::Reqwest(value)
    }
}
