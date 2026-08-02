use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const DEFAULT_IPC_PORT: u16 = 7776;
pub const DEFAULT_MCP_PORT: u16 = 7777;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub address: SocketAddr,
    pub mcp_address: SocketAddr,
    pub tick_rate_hz: u32,
    pub status_publish_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_IPC_PORT),
            mcp_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), DEFAULT_MCP_PORT),
            tick_rate_hz: 20,
            status_publish_interval_ms: 1_000,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.address.ip().is_loopback() {
            return Err(ConfigError::NonLoopback {
                name: "IPC",
                address: self.address,
            });
        }
        if !self.mcp_address.ip().is_loopback() {
            return Err(ConfigError::NonLoopback {
                name: "MCP",
                address: self.mcp_address,
            });
        }
        if self.address == self.mcp_address && self.address.port() != 0 {
            return Err(ConfigError::AddressConflict(self.address));
        }
        if !(1..=1_000).contains(&self.tick_rate_hz) {
            return Err(ConfigError::InvalidTickRate(self.tick_rate_hz));
        }
        if !(50..=60_000).contains(&self.status_publish_interval_ms) {
            return Err(ConfigError::InvalidPublishInterval(
                self.status_publish_interval_ms,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub runtime_file: PathBuf,
    pub log_file: PathBuf,
}

impl AppPaths {
    pub fn discover(config_override: Option<PathBuf>) -> Result<Self, ConfigError> {
        let project = ProjectDirs::from("", "", "substrate").ok_or(ConfigError::NoHomeDirectory)?;
        let config_file = config_override
            .or_else(|| env::var_os("SUBSTRATE_CONFIG").map(PathBuf::from))
            .unwrap_or_else(|| project.config_dir().join("config.toml"));
        let data_dir = env::var_os("SUBSTRATE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| project.data_local_dir().to_path_buf());
        Ok(Self {
            config_file,
            runtime_file: data_dir.join("daemon.json"),
            log_file: data_dir.join("mind-daemon.log"),
            data_dir,
        })
    }

    pub fn ensure_directories(&self) -> Result<(), ConfigError> {
        if let Some(parent) = self.config_file.parent() {
            fs::create_dir_all(parent).map_err(|source| ConfigError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::create_dir_all(&self.data_dir).map_err(|source| ConfigError::Io {
            path: self.data_dir.clone(),
            source,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct ConfigOverrides {
    pub config_path: Option<PathBuf>,
    pub address: Option<SocketAddr>,
    pub mcp_address: Option<SocketAddr>,
}

pub fn load_or_create(overrides: ConfigOverrides) -> Result<(Config, AppPaths), ConfigError> {
    let paths = AppPaths::discover(overrides.config_path)?;
    paths.ensure_directories()?;

    let mut config = if paths.config_file.exists() {
        let raw = fs::read_to_string(&paths.config_file).map_err(|source| ConfigError::Io {
            path: paths.config_file.clone(),
            source,
        })?;
        toml::from_str(&raw).map_err(|source| ConfigError::Parse {
            path: paths.config_file.clone(),
            source,
        })?
    } else {
        let config = Config::default();
        write_config(&paths.config_file, &config)?;
        config
    };

    if let Ok(address) = env::var("SUBSTRATE_ADDR") {
        config.address = address
            .parse()
            .map_err(|_| ConfigError::InvalidAddress(address))?;
    }
    if let Ok(rate) = env::var("SUBSTRATE_TICK_RATE_HZ") {
        config.tick_rate_hz = rate
            .parse()
            .map_err(|_| ConfigError::InvalidEnvironment("SUBSTRATE_TICK_RATE_HZ"))?;
    }
    if let Ok(address) = env::var("SUBSTRATE_MCP_ADDR") {
        config.mcp_address = address
            .parse()
            .map_err(|_| ConfigError::InvalidMcpAddress(address))?;
    }
    if let Ok(interval) = env::var("SUBSTRATE_STATUS_INTERVAL_MS") {
        config.status_publish_interval_ms = interval
            .parse()
            .map_err(|_| ConfigError::InvalidEnvironment("SUBSTRATE_STATUS_INTERVAL_MS"))?;
    }
    if let Some(address) = overrides.address {
        config.address = address;
    }
    if let Some(address) = overrides.mcp_address {
        config.mcp_address = address;
    }
    config.validate()?;
    Ok((config, paths))
}

fn write_config(path: &Path, config: &Config) -> Result<(), ConfigError> {
    let raw = toml::to_string_pretty(config).map_err(ConfigError::Serialize)?;
    fs::write(path, raw).map_err(|source| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not determine a platform configuration directory")]
    NoHomeDirectory,
    #[error("{name} address must be loopback, got {address}")]
    NonLoopback {
        name: &'static str,
        address: SocketAddr,
    },
    #[error("IPC and MCP must use different ports, both were configured as {0}")]
    AddressConflict(SocketAddr),
    #[error("invalid socket address in SUBSTRATE_ADDR: {0}")]
    InvalidAddress(String),
    #[error("invalid socket address in SUBSTRATE_MCP_ADDR: {0}")]
    InvalidMcpAddress(String),
    #[error("invalid value for {0}")]
    InvalidEnvironment(&'static str),
    #[error("tick_rate_hz must be between 1 and 1000, got {0}")]
    InvalidTickRate(u32),
    #[error("status_publish_interval_ms must be between 50 and 60000, got {0}")]
    InvalidPublishInterval(u64),
    #[error("failed to read or write {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to serialize configuration: {0}")]
    Serialize(toml::ser::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_public_bind_addresses() {
        let config = Config {
            address: "0.0.0.0:7776".parse().unwrap(),
            ..Config::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigError::NonLoopback { .. })
        ));
    }

    #[test]
    fn rejects_public_mcp_bind_addresses() {
        let config = Config {
            mcp_address: "0.0.0.0:7777".parse().unwrap(),
            ..Config::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigError::NonLoopback { name: "MCP", .. })
        ));
    }

    #[test]
    fn round_trips_toml() {
        let config = Config::default();
        let encoded = toml::to_string(&config).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn rejects_shared_nonzero_port() {
        let config = Config {
            mcp_address: Config::default().address,
            ..Config::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ConfigError::AddressConflict(_))
        ));
    }
}
