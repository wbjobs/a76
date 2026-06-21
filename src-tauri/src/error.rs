use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Process not found: {0}")]
    ProcessNotFound(String),
    #[error("Cannot open process (pid={pid}): {source}")]
    CannotOpenProcess {
        pid: u32,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("Memory read failed at 0x{addr:X}: {source}")]
    MemoryReadFailed {
        addr: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("Memory write failed at 0x{addr:X}: {source}")]
    MemoryWriteFailed {
        addr: u64,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("Memory region scan failed: {0}")]
    MemoryScanFailed(String),
    #[error("Database error: {0}")]
    DatabaseError(#[from] rusqlite::Error),
    #[error("Invalid snapshot id: {0}")]
    InvalidSnapshotId(i64),
    #[error("No active process selected")]
    NoActiveProcess,
    #[error("Hotkey registration failed: {0}")]
    HotkeyError(String),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Regex error: {0}")]
    RegexError(#[from] regex::Error),
    #[error("Other: {0}")]
    Other(String),
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
