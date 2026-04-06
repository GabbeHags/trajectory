use std::{collections::HashMap, fs, path::Path};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file")]
    IO,
    #[error("failed to parse config file")]
    Parse,
    #[error("configuration validation failed")]
    Validation,
}

/// Marker trait for config states
pub trait ConfigState: std::fmt::Debug {}

/// Unverified state - config loaded but not yet validated
#[derive(Debug)]
pub struct Unverified;

/// Verified state - config is validated and ready to use
#[derive(Debug)]
pub struct Verified;

impl ConfigState for Unverified {}
impl ConfigState for Verified {}

/// Config with state pattern
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config<S: ConfigState> {
    entry_points: HashMap<String, EntryPoint>,
    routers: HashMap<String, Router>,
    services: HashMap<String, Service>,

    #[serde(skip)]
    _state: std::marker::PhantomData<S>,
}

/// Methods available in Unverified state
impl Config<Unverified> {
    /// Load config from file, returns Unverified state
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Failed to read config file {}: {}", path.display(), e);
                return Err(ConfigError::IO);
            }
        };

        let config = match toml::from_str::<Config<Unverified>>(&content) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Failed to parse config file {}: {}", path.display(), e);
                return Err(ConfigError::Parse);
            }
        };

        Ok(config)
    }

    /// Verify Config
    pub fn verify(self) -> Result<Config<Verified>, ConfigError> {
        // Validation logic here
        if self.entry_points.is_empty() {
            log::error!("No entry points configured");
            return Err(ConfigError::Validation);
        }
        if self.routers.is_empty() {
            log::error!("No routers configured");
            return Err(ConfigError::Validation);
        }
        if self.services.is_empty() {
            log::error!("No services configured");
            return Err(ConfigError::Validation);
        }

        // Verify that all routers reference valid entry points and services
        for (name, router) in &self.routers {
            if !self.entry_points.contains_key(&router.entry_point) {
                log::error!(
                    "Router '{}' references unknown entry point '{}'",
                    name,
                    router.entry_point
                );
                return Err(ConfigError::Validation);
            }
            if !self.services.contains_key(&router.service) {
                log::error!(
                    "Router '{}' references unknown service '{}'",
                    name,
                    router.service
                );
                return Err(ConfigError::Validation);
            }
        }

        Ok(Config {
            entry_points: self.entry_points,
            routers: self.routers,
            services: self.services,

            _state: std::marker::PhantomData,
        })
    }
}

impl Config<Verified> {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    HTTP,
    HTTPS,
    TCP,
    UDP,
}

#[derive(Debug, Deserialize)]
pub struct EntryPoint {
    port: u16,
    protocol: Protocol,
}

#[derive(Debug, Deserialize)]
pub struct Router {
    entry_point: String,
    #[serde(rename = "match")]
    match_rule: String,
    service: String,
}

#[derive(Debug, Deserialize)]
pub struct Service {
    servers: Vec<String>,
}
