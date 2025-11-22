//Parses configuration file
use std::fs;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Route {
    pub path: String,
    pub methods: Vec<String>,
    pub root: Option<String>,
    pub default_file: Option<String>,
    pub cgi_extensions: Option<Vec<String>>,
    pub max_body_size: Option<u64>,
    pub directory_listing: Option<bool>,
    pub redirect: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Server {
    pub host: String,
    pub ports: Vec<u16>,
    pub server_name: String,
    pub error_pages: Option<std::collections::HashMap<u16, String>>,
    pub max_body_size: Option<u64>,
    pub routes: Vec<Route>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub servers: Vec<Server>,
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        for server in &self.servers {
            if server.ports.is_empty() {
                return Err(format!("Server {} has no ports", server.server_name));
            }
            for port in &server.ports {
                if *port == 0 {
                    return Err(format!(
                        "Invalid port {} for server {}",
                        port, server.server_name
                    ));
                }
            }
            for route in &server.routes {
                if !route.path.starts_with('/') {
                    return Err(format!("Route path {} must start with /", route.path));
                }
                for method in &route.methods {
                    if !["GET", "POST", "DELETE"].contains(&method.as_str()) {
                        return Err(format!("Invalid method {} on route {}", method, route.path));
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn parses_configuration_file() -> Result<Config, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string("./config.yaml")?;
    let config: Config = serde_yaml::from_str(&contents)?;
    for server in config.servers.iter() {
        println!("{}", server.host);
    }

    match config.validate() {
        Ok(_) => {
            println!("Configuration is valid! Server can start.");
        }
        Err(e) => {
            eprintln!("Configuration validation error: {}", e);
            std::process::exit(1);
        }
    }
    
    Ok(config)
}
