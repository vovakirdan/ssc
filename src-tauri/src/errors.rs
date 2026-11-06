use serde::Serialize;
use std::fmt;

/// Safe to show in the UI and include in diagnostics. Never put external input here.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AppError {
    pub code: String,
    pub stage: String,
    pub message: String,
    pub retryable: bool,
}

impl AppError {
    pub fn new(code: &str, stage: &str, message: &str, retryable: bool) -> Self {
        Self {
            code: code.into(),
            stage: stage.into(),
            message: message.into(),
            retryable,
        }
    }
}
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AppError {}
