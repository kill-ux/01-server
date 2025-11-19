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

#[derive(Debug, Deserialize,  Clone)]
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

pub fn parses_configuration_file() -> Result<Config, Box<dyn std::error::Error>> {   
    let contents = fs::read_to_string("./config.yaml")?;
    let config: Config = serde_yaml::from_str(&contents)?;
    for server in config.servers.iter() {
        println!("{}", server.host);
    }
    Ok(config)
}