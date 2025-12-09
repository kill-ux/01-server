use std::{error::Error, fs::File, io::Read};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub enum Method {
    GET,
    POST,
    DELETE
}

#[derive(Deserialize, Debug)]
pub struct Route {
    pub path: String,
    pub methods: Vec<Method>
}

#[derive(Deserialize, Debug)]
pub struct Server {
    pub host: String,
    pub ports: Vec<u16>,
    pub server_name: String,
    pub routes: Vec<Route>
}

#[derive(Deserialize, Debug)]
/// Configuration
pub struct Config {
    pub servers: Vec<Server>,
}

impl Config {
    pub fn parse() -> Result<Config, Box<dyn Error>> {
        let mut file = File::open("config.yaml")?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        let config: Config = serde_yaml::from_str(&buf)?;
        Ok(config)
    }
}
