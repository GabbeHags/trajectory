use std::{collections::HashMap, fs, path::Path, sync::OnceLock};

use serde::Deserialize;

static GLOBAL_CONFIG: OnceLock<Config<Verified>> = OnceLock::new();

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

    /// Verify entry points are not empty
    fn verify_entry_points_not_empty(&self) -> Result<(), ConfigError> {
        if self.entry_points.is_empty() {
            log::error!("No entry points configured");
            return Err(ConfigError::Validation);
        }
        Ok(())
    }

    /// Verify routers are not empty
    fn verify_routers_not_empty(&self) -> Result<(), ConfigError> {
        if self.routers.is_empty() {
            log::error!("No routers configured");
            return Err(ConfigError::Validation);
        }
        Ok(())
    }

    /// Verify services are not empty
    fn verify_services_not_empty(&self) -> Result<(), ConfigError> {
        if self.services.is_empty() {
            log::error!("No services configured");
            return Err(ConfigError::Validation);
        }
        Ok(())
    }

    /// Verify all entry point ports are unique
    fn verify_unique_ports(&self) -> Result<(), ConfigError> {
        let mut port_to_entries: std::collections::HashMap<u16, Vec<String>> =
            std::collections::HashMap::new();

        for (name, ep) in &self.entry_points {
            port_to_entries
                .entry(ep.port())
                .or_default()
                .push(name.clone());
        }

        let mut duplicates_found = false;
        for (port, entries) in port_to_entries {
            if entries.len() > 1 {
                duplicates_found = true;
                log::error!(
                    "Port {} is used by multiple entry points: {}",
                    port,
                    entries.join(", ")
                );
            }
        }

        if duplicates_found {
            return Err(ConfigError::Validation);
        }

        Ok(())
    }

    /// Verify all routers reference valid entry points and services
    fn verify_router_references(&self) -> Result<(), ConfigError> {
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
        Ok(())
    }

    /// Verify Config
    pub fn verify(self) -> Result<Config<Verified>, ConfigError> {
        self.verify_entry_points_not_empty()?;
        self.verify_routers_not_empty()?;
        self.verify_services_not_empty()?;
        self.verify_unique_ports()?;
        self.verify_router_references()?;

        Ok(Config {
            entry_points: self.entry_points,
            routers: self.routers,
            services: self.services,

            _state: std::marker::PhantomData,
        })
    }
}

impl Config<Verified> {
    pub fn get_entry_points(&self) -> &HashMap<String, EntryPoint> {
        &self.entry_points
    }

    pub fn get_routers(&self) -> &HashMap<String, Router> {
        &self.routers
    }

    pub fn get_services(&self) -> &HashMap<String, Service> {
        &self.services
    }

    /// Build routing map: entry_point_name -> Vec<RouteInfo>
    pub fn build_routing_table(&self) -> HashMap<String, Vec<RouteMapping>> {
        let mut routing_map: HashMap<String, Vec<RouteMapping>> = HashMap::new();

        for router in self.routers.values() {
            if let (Some(entry_point), Some(service)) = (
                self.entry_points.get(&router.entry_point),
                self.services.get(&router.service),
            ) {
                let route_info = RouteMapping {
                    entry_point: entry_point.clone(),
                    match_rule: router.match_rule.clone(),
                    service: service.clone(),
                };

                routing_map
                    .entry(router.entry_point.clone())
                    .or_default()
                    .push(route_info);
            }
        }

        routing_map
    }
}

/// Initialize the global config
pub fn init_config(config: Config<Verified>) {
    GLOBAL_CONFIG
        .set(config)
        .expect("Global config already initialized");
}

/// Get the global config
pub fn get_config() -> &'static Config<Verified> {
    GLOBAL_CONFIG
        .get()
        .expect("Config not initialized. Call init_config() first.")
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Http,
    Https,
    Tcp,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EntryPoint {
    port: u16,
    protocol: Protocol,
}

impl EntryPoint {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn protocol(&self) -> Protocol {
        self.protocol
    }
}

#[derive(Debug, Deserialize)]
pub struct Router {
    entry_point: String,
    #[serde(rename = "match")]
    match_rule: String,
    service: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Service {
    servers: Vec<String>,
}

impl Service {
    pub fn servers(&self) -> &[String] {
        &self.servers
    }
}

/// Route information for an entry point
#[derive(Debug, Clone)]
pub struct RouteMapping {
    pub entry_point: EntryPoint,
    pub match_rule: String,
    pub service: Service,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_valid_config() -> Config<Unverified> {
        let mut entry_points = HashMap::new();
        entry_points.insert(
            "main".to_string(),
            EntryPoint {
                port: 8080,
                protocol: Protocol::Http,
            },
        );

        let mut routers = HashMap::new();
        routers.insert(
            "api".to_string(),
            Router {
                entry_point: "main".to_string(),
                match_rule: "/api/*".to_string(),
                service: "backend".to_string(),
            },
        );

        let mut services = HashMap::new();
        services.insert(
            "backend".to_string(),
            Service {
                servers: vec!["127.0.0.1:9000".to_string()],
            },
        );

        Config {
            entry_points,
            routers,
            services,
            _state: std::marker::PhantomData,
        }
    }

    #[test]
    fn test_valid_config_verification() {
        let config = create_valid_config();
        assert!(config.verify().is_ok());
    }

    #[test]
    fn test_empty_entry_points_validation() {
        let mut config = create_valid_config();
        config.entry_points.clear();
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_empty_routers_validation() {
        let mut config = create_valid_config();
        config.routers.clear();
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_empty_services_validation() {
        let mut config = create_valid_config();
        config.services.clear();
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_router_invalid_entry_point_reference() {
        let mut config = create_valid_config();
        config.routers.insert(
            "invalid_router".to_string(),
            Router {
                entry_point: "nonexistent".to_string(),
                match_rule: "/test/*".to_string(),
                service: "backend".to_string(),
            },
        );
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_router_invalid_service_reference() {
        let mut config = create_valid_config();
        config.routers.insert(
            "invalid_router".to_string(),
            Router {
                entry_point: "main".to_string(),
                match_rule: "/test/*".to_string(),
                service: "nonexistent_service".to_string(),
            },
        );
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_multiple_entry_points() {
        let mut config = create_valid_config();
        config.entry_points.insert(
            "secondary".to_string(),
            EntryPoint {
                port: 8443,
                protocol: Protocol::Https,
            },
        );
        assert!(config.verify().is_ok());
    }

    #[test]
    fn test_multiple_routers() {
        let mut config = create_valid_config();
        config.routers.insert(
            "web".to_string(),
            Router {
                entry_point: "main".to_string(),
                match_rule: "/*".to_string(),
                service: "backend".to_string(),
            },
        );
        assert!(config.verify().is_ok());
    }

    #[test]
    fn test_duplicate_entry_point_ports() {
        let mut config = create_valid_config();
        config.entry_points.insert(
            "duplicate".to_string(),
            EntryPoint {
                port: 8080, // Same port as "main"
                protocol: Protocol::Http,
            },
        );
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_multiple_duplicate_ports() {
        let mut config = create_valid_config();
        config.entry_points.insert(
            "duplicate1".to_string(),
            EntryPoint {
                port: 8080, // Same port as "main"
                protocol: Protocol::Http,
            },
        );
        config.entry_points.insert(
            "duplicate2".to_string(),
            EntryPoint {
                port: 8080, // Also same port as "main"
                protocol: Protocol::Https,
            },
        );
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_multiple_protocols_different_ports() {
        let mut config = create_valid_config();
        config.entry_points.insert(
            "https_ep".to_string(),
            EntryPoint {
                port: 8443,
                protocol: Protocol::Https,
            },
        );
        config.entry_points.insert(
            "tcp_ep".to_string(),
            EntryPoint {
                port: 9000,
                protocol: Protocol::Tcp,
            },
        );
        assert!(config.verify().is_ok());
    }

    #[test]
    fn test_multiple_routers_one_invalid_entry_point() {
        let mut config = create_valid_config();
        config.routers.insert(
            "secondary".to_string(),
            Router {
                entry_point: "nonexistent_ep".to_string(),
                match_rule: "/secondary/*".to_string(),
                service: "backend".to_string(),
            },
        );
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_multiple_routers_one_invalid_service() {
        let mut config = create_valid_config();
        config.routers.insert(
            "secondary".to_string(),
            Router {
                entry_point: "main".to_string(),
                match_rule: "/secondary/*".to_string(),
                service: "nonexistent_service".to_string(),
            },
        );
        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_mixed_valid_protocols() {
        let mut entry_points = HashMap::new();
        entry_points.insert(
            "http".to_string(),
            EntryPoint {
                port: 8080,
                protocol: Protocol::Http,
            },
        );
        entry_points.insert(
            "https".to_string(),
            EntryPoint {
                port: 8443,
                protocol: Protocol::Https,
            },
        );
        entry_points.insert(
            "tcp".to_string(),
            EntryPoint {
                port: 9000,
                protocol: Protocol::Tcp,
            },
        );

        let mut routers = HashMap::new();
        routers.insert(
            "api".to_string(),
            Router {
                entry_point: "http".to_string(),
                match_rule: "/api/*".to_string(),
                service: "backend".to_string(),
            },
        );

        let mut services = HashMap::new();
        services.insert(
            "backend".to_string(),
            Service {
                servers: vec!["127.0.0.1:9000".to_string()],
            },
        );

        let config = Config {
            entry_points,
            routers,
            services,
            _state: std::marker::PhantomData,
        };

        assert!(config.verify().is_ok());
    }

    #[test]
    fn test_multiple_different_duplicate_ports() {
        let mut entry_points = HashMap::new();
        entry_points.insert(
            "http1".to_string(),
            EntryPoint {
                port: 8080,
                protocol: Protocol::Http,
            },
        );
        entry_points.insert(
            "http2".to_string(),
            EntryPoint {
                port: 8080, // Duplicate
                protocol: Protocol::Http,
            },
        );
        entry_points.insert(
            "https1".to_string(),
            EntryPoint {
                port: 9000,
                protocol: Protocol::Https,
            },
        );
        entry_points.insert(
            "https2".to_string(),
            EntryPoint {
                port: 9000, // Duplicate
                protocol: Protocol::Https,
            },
        );

        let mut routers = HashMap::new();
        routers.insert(
            "api".to_string(),
            Router {
                entry_point: "http1".to_string(),
                match_rule: "/api/*".to_string(),
                service: "backend".to_string(),
            },
        );

        let mut services = HashMap::new();
        services.insert(
            "backend".to_string(),
            Service {
                servers: vec!["127.0.0.1:9000".to_string()],
            },
        );

        let config = Config {
            entry_points,
            routers,
            services,
            _state: std::marker::PhantomData,
        };

        assert!(matches!(config.verify(), Err(ConfigError::Validation)));
    }

    #[test]
    fn test_build_routing_table() {
        let config = create_valid_config();
        let verified_config = config.verify().unwrap();
        let routing_map = verified_config.build_routing_table();

        // Verify that main entry point has the api router
        assert!(routing_map.contains_key("main"));
        let routes = &routing_map["main"];
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].entry_point.port(), 8080);
        assert_eq!(routes[0].match_rule, "/api/*");
        assert_eq!(
            routes[0].service.servers,
            vec!["127.0.0.1:9000".to_string()]
        );
    }

    #[test]
    fn test_build_routing_table_multiple_routers() {
        let mut config = create_valid_config();
        config.routers.insert(
            "web".to_string(),
            Router {
                entry_point: "main".to_string(),
                match_rule: "/*".to_string(),
                service: "backend".to_string(),
            },
        );

        let verified_config = config.verify().unwrap();
        let routing_map = verified_config.build_routing_table();

        // Verify that main entry point has multiple routes
        assert!(routing_map.contains_key("main"));
        let routes = &routing_map["main"];
        assert_eq!(routes.len(), 2);

        // Check both routes are present
        let match_rules: Vec<&String> = routes.iter().map(|r| &r.match_rule).collect();
        assert!(match_rules.contains(&&"/api/*".to_string()));
        assert!(match_rules.contains(&&"/*".to_string()));
    }
}
