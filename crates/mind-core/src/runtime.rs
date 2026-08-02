use crate::PROTOCOL_VERSION;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{fs, io, net::SocketAddr, path::Path};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMetadata {
    pub pid: u32,
    pub address: SocketAddr,
    pub protocol_version: u32,
    pub started_at: DateTime<Utc>,
}

impl RuntimeMetadata {
    pub fn current(address: SocketAddr, started_at: DateTime<Utc>) -> Self {
        Self {
            pid: std::process::id(),
            address,
            protocol_version: PROTOCOL_VERSION,
            started_at,
        }
    }

    pub fn read(path: &Path) -> Result<Option<Self>, RuntimeError> {
        match fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(RuntimeError::Io(error)),
        }
    }

    pub fn write_atomic(&self, path: &Path) -> Result<(), RuntimeError> {
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn remove(path: &Path) -> Result<(), RuntimeError> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RuntimeError::Io(error)),
        }
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime metadata I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid runtime metadata: {0}")]
    Json(#[from] serde_json::Error),
}
