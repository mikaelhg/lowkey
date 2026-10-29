//! Typed server configuration.
//!
//! Configuration is read from an optional TOML file and then overridden by
//! environment variables. Every setting has a sensible default, so the server
//! starts without any configuration at all.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Environment variable naming the config file.
pub const CONFIG_PATH_VAR: &str = "LOWKEY_CONFIG";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Address and port to listen on.
    pub listen: SocketAddr,
    /// Directory holding all user data.
    pub data_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: SocketAddr::from(([127, 0, 0, 1], 8080)),
            data_dir: PathBuf::from("data"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read config file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config file {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("invalid value for {var}: {value}")]
    Env { var: &'static str, value: String },
}

impl Config {
    /// Loads the config file named by `LOWKEY_CONFIG`, if set, and applies
    /// environment overrides.
    pub fn load() -> Result<Self, ConfigError> {
        let config = match std::env::var_os(CONFIG_PATH_VAR) {
            Some(path) => Self::from_file(Path::new(&path))?,
            None => Self::default(),
        };
        config.with_env_overrides(|var| std::env::var(var).ok())
    }

    /// Reads and parses a TOML config file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_owned(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_owned(),
            source,
        })
    }

    /// Applies `LOWKEY_LISTEN` and `LOWKEY_DATA_DIR` overrides. The lookup is
    /// passed in so tests don't have to touch the process environment.
    pub fn with_env_overrides(
        mut self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        if let Some(value) = lookup("LOWKEY_LISTEN") {
            self.listen = value.parse().map_err(|_| ConfigError::Env {
                var: "LOWKEY_LISTEN",
                value,
            })?;
        }
        if let Some(value) = lookup("LOWKEY_DATA_DIR") {
            self.data_dir = PathBuf::from(value);
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn file_values_are_read() {
        let config: Config =
            toml::from_str("listen = \"0.0.0.0:9000\"\ndata_dir = \"/var/lib/lowkey\"").unwrap();
        assert_eq!(config.listen, "0.0.0.0:9000".parse().unwrap());
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/lowkey"));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        assert!(toml::from_str::<Config>("lisen = \"0.0.0.0:9000\"").is_err());
    }

    #[test]
    fn env_overrides_file_values() {
        let config = Config::default()
            .with_env_overrides(|var| match var {
                "LOWKEY_LISTEN" => Some("[::1]:7000".into()),
                "LOWKEY_DATA_DIR" => Some("/tmp/lk".into()),
                _ => None,
            })
            .unwrap();
        assert_eq!(config.listen, "[::1]:7000".parse().unwrap());
        assert_eq!(config.data_dir, PathBuf::from("/tmp/lk"));
    }

    #[test]
    fn invalid_env_value_is_an_error() {
        let result = Config::default().with_env_overrides(|var| {
            (var == "LOWKEY_LISTEN").then(|| "not an address".to_owned())
        });
        assert!(matches!(result, Err(ConfigError::Env { .. })));
    }
}
