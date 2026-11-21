use crate::PROTOCOL_VERSION;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    net::SocketAddr,
    path::Path,
};
use tempfile::NamedTempFile;
use thiserror::Error;

fn default_mcp_address() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 7777))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMetadata {
    pub pid: u32,
    pub address: SocketAddr,
    #[serde(default = "default_mcp_address")]
    pub mcp_address: SocketAddr,
    pub protocol_version: u32,
    pub started_at: DateTime<Utc>,
}

impl RuntimeMetadata {
    pub fn current(
        address: SocketAddr,
        mcp_address: SocketAddr,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            pid: std::process::id(),
            address,
            mcp_address,
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
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut temporary = NamedTempFile::new_in(parent)?;
        temporary.write_all(&serde_json::to_vec_pretty(self)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| RuntimeError::Io(error.error))?;
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
