use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ForceClickError {
    #[error("no T2 precision trackpad input device found")]
    NoTrackpadDevice,
    #[error("io error at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("failed to spawn process: {0}")]
    ProcessSpawn(io::Error),
    #[error("command failed: {command}: {stderr}")]
    CommandFailed { command: String, stderr: String },
    #[error("protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, ForceClickError>;
