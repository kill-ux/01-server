use std::{collections::HashMap, iter::Peekable, str::Lines};

// #[derive(Deserialize, Debug)]
// pub struct Route {
//     pub path: String,
//     pub methods: Vec<Method>,
// }

// #[derive(Deserialize, Debug)]
// pub struct Server {
//     pub host: String,
//     pub ports: Vec<u16>,
//     pub server_name: String,
//     pub routes: Vec<Route>,
// }

// #[derive(Deserialize, Debug)]
// /// Configuration
// pub struct Config {
//     pub servers: Vec<Server>,
// }

// impl Config {
//     pub fn parse() -> Result<Config, Box<dyn Error>> {
//         let mut file = File::open("config.yaml")?;
//         let mut buf = String::new();
//         file.read_to_string(&mut buf)?;
//         let config: Config = serde_yaml::from_str(&buf)?;
//         Ok(config)
//     }
// }

#[derive(Debug, Clone)]
pub struct RouteConfig {
    pub methods: Vec<String>,
    pub redirection: Option<String>,
    pub root: String,
    pub default_file: String,
    pub cgi_ext: Option<String>,
    pub autoindex: bool,
    pub client_max_body_size: usize,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub ports: Vec<u16>, // Multiple ports
    pub server_names: Vec<String>,
    pub default_server: bool,              // Default server selection
    pub error_pages: HashMap<u16, String>, // Custom error page paths
    pub client_max_body_size: usize,
    pub routes: HashMap<String, RouteConfig>,
}

#[derive(Debug, Default)]
pub struct AppConfig {
    pub servers: Vec<ServerConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            ports: Vec::new(),
            server_names: Vec::new(),
            default_server: false,
            error_pages: HashMap::new(),
            client_max_body_size: 1024 * 1024, // 1MB default
            routes: HashMap::new(),
        }
    }
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            methods: vec!["GET".to_string()],
            redirection: None,
            root: String::new(),
            default_file: "index.html".to_string(),
            cgi_ext: None,
            autoindex: false,
            client_max_body_size: 1024 * 1024,
        }
    }
}

impl AppConfig {
    pub fn parse_str(content: &str) -> Self {
        let mut config = AppConfig::default();
        let mut lines = content.lines().peekable();
        while let Some(line) = lines.next() {
            let trimmed = line.trim();
            if trimmed == "serevr:" {
                config.servers.push(Self::parse_server(&mut lines));
            }
        }
        config
    }

    fn parse_server(lines: &mut Peekable<Lines>) -> ServerConfig {
        let mut server = ServerConfig::default();

        while let Some(line) = lines.peek() {
            let indent = line.len() - line.trim_start().len();
            let trimmed = line.trim();
            if indent == 0 && !trimmed.is_empty() && trimmed != "server:" {
                break;
            }
            let line_to_process = lines.next().unwrap().trim();
            if let Some((key, value)) = line_to_process.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                match key {
                    "host" => server.host = value.to_string(),
                    "ports" => {
                        server.ports = value
                            .trim_matches(|c| c == '[' || c == ']')
                            .split(',')
                            .filter_map(|s| s.trim().parse().ok())
                            .collect()
                    }
                    "server_names" => server.server_names = value.split_whitespace().map(|s| s.to_string()).collect(),
                    "default_server" => server.default_server = value == "true",
                    "client_max_body_size" => server.client_max_body_size = value.parse().unwrap_or(1024),

                    _ => {}
                }
            }
        }
        server
    }
}
