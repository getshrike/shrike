//! Library errors. The engine maps these to structured protocol errors.

use thiserror::Error;

pub type ShrikeResult<T> = Result<T, ShrikeError>;

#[derive(Debug, Error)]
pub enum ShrikeError {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("invalid protocol message: {0}")]
    Protocol(String),
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("snapshot error: {0}")]
    Snapshot(String),
    #[error("resolution error: {0}")]
    Resolve(String),
    #[error("analysis error: {0}")]
    Analysis(String),
    #[error("incomparable snapshots: {0}")]
    Incomparable(String),
    #[error("build evidence error: {0}")]
    Evidence(String),
}

impl ShrikeError {
    /// Stable machine-readable error code used in the protocol envelope.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Config(_) => "config_error",
            Self::Protocol(_) => "protocol_error",
            Self::UnknownCommand(_) => "unknown_command",
            Self::Io(_) => "io_error",
            Self::Json(_) => "json_error",
            Self::Snapshot(_) => "snapshot_error",
            Self::Resolve(_) => "resolve_error",
            Self::Analysis(_) => "analysis_error",
            Self::Incomparable(_) => "incomparable",
            Self::Evidence(_) => "evidence_error",
        }
    }
}
