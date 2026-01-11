use derive_yaml::FromYaml;
use std::{clone, collections::HashMap};
use crate::config::parser::FromYaml; // Import trait

// --- Constants ---
pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_SERVER_NAME: &str = "_";
pub const DEFAULT_MAX_BODY_SIZE: usize = 1_048_576; // 1MB
pub const DEFAULT_ROUTE_PATH: &str = "/";
pub const DEFAULT_ROOT: &str = "./www";
pub const DEFAULT_FILE: &str = "index.html";

#[derive(Debug,Clone , FromYaml)]
pub struct RouteConfig {
    pub path: String,
    pub methods: Vec<String>,
    pub redirection: Option<String>,
    pub root: String,
    pub default_file: String,
    pub cgi_ext: Option<String>,
    pub autoindex: bool,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            path: DEFAULT_ROUTE_PATH.to_string(),
            methods: vec!["GET".to_string(), "HEAD".to_string()],
            redirection: None,
            root: DEFAULT_ROOT.to_string(),
            default_file: DEFAULT_FILE.to_string(),
            cgi_ext: None,
            autoindex: false,
        }
    }
}

#[derive(Debug, Clone, FromYaml)]
pub struct Config {
    pub servers: Vec<ServerConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            servers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, FromYaml)]
pub struct ServerConfig {
    pub host: String,
    pub ports: Vec<u16>,
    pub server_name: String,
    pub default_server: bool,
    pub error_pages: HashMap<u16, String>,
    pub client_max_body_size: usize,
    pub routes: Vec<RouteConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            ports: vec![DEFAULT_PORT],
            server_name: DEFAULT_SERVER_NAME.to_string(),
            default_server: false,
            error_pages: HashMap::new(),
            client_max_body_size: DEFAULT_MAX_BODY_SIZE,
            routes: Vec::new(),
        }
    }
}
