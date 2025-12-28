use std::{collections::HashMap, fs::read_to_string, iter::Peekable, str::Lines};

use parser::lexer::{Token, Tokenizer};

use crate::error::Result;
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
    pub server_name: String,
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
            server_name: String::new(),
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
    pub fn parse() -> Result<Self> {
        let config_content = read_to_string("config.yaml")?;
        let config = Self::parse_str(&config_content);
        Ok(config)
    }

    pub fn parse_str(content: &str) -> Self {
        let mut config = AppConfig::default();
        let mut lines = content.lines().peekable();

        while let Some(line) = lines.peek() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                lines.next();
                continue;
            }

            if trimmed == "server:" {
                lines.next();
                config.servers.push(Self::parse_server(&mut lines));
            } else {
                lines.next(); // Skip comments/empty lines between servers
            }
        }
        config
    }

    fn clean_val(val: &str) -> String {
        val.trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string()
    }

    fn parse_server(lines: &mut Peekable<Lines>) -> ServerConfig {
        let mut server = ServerConfig::default();

        while let Some(line) = lines.peek() {
            let indent = line.len() - line.trim_start().len();
            let trimmed = line.trim();
            if indent == 0 && !trimmed.is_empty() {
                break;
            }
            let line_to_process = lines.next().unwrap().trim();
            if let Some((key, raw_val)) = line_to_process.split_once(':') {
                let val = Self::clean_val(raw_val);
                // let val = val.as_str();

                match key.trim() {
                    "host" => server.host = val.to_string(),
                    "ports" => {
                        server.ports = Self::parse_list(lines, &val, indent + 2)
                            .iter()
                            .filter_map(|p| {
                                let port = p.parse::<u16>();
                                if port.is_err() {
                                    eprintln!("Warning: Invalid port '{}' skipped", p);
                                }
                                port.ok()
                            })
                            .collect()
                    }
                    "server_name" => server.server_name = Self::clean_val(&val),
                    "default_server" => server.default_server = val == "true",
                    "client_max_body_size" => {
                        server.client_max_body_size = val.parse().unwrap_or(1024)
                    }
                    "routes" => server.routes = Self::parse_routes_map(lines),
                    "error_pages" => {
                        if val.is_empty() {
                            // Block style (nested lines)
                            server.error_pages = Self::parse_error_pages(lines, indent + 2);
                        } else {
                            // Inline style: {404: /4.html, 500: /5.html}
                            let clean = val.trim_matches(|c| c == '{' || c == '}');
                            for pair in clean.split(',') {
                                if let Some((k, v)) = pair.split_once(':')
                                    && let Ok(code) = k.trim().parse::<u16>()
                                {
                                    server.error_pages.insert(code, Self::clean_val(v));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        server
    }

    fn parse_list(
        lines: &mut Peekable<Lines>,
        inline_val: &str,
        required_indent: usize,
    ) -> Vec<String> {
        if !inline_val.is_empty() {
            // Handle: [GET, POST] or val1 val2
            return inline_val
                .trim_matches(|c| c == '[' || c == ']')
                .split([',', ' '])
                .map(Self::clean_val)
                .filter(|s| !s.is_empty())
                .collect();
        }

        let mut list = Vec::new();
        while let Some(line) = lines.peek() {
            let current_indent = line.len() - line.trim_start().len();
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                lines.next();
                continue;
            }

            if current_indent < required_indent {
                break;
            }

            let line_data = lines.next().unwrap().trim();
            if line_data.starts_with("- ") {
                list.push(Self::clean_val(line_data.trim_start_matches("- ")));
            }
        }
        list
    }

    fn parse_error_pages(
        lines: &mut Peekable<Lines>,
        required_indent: usize,
    ) -> HashMap<u16, String> {
        let mut map = HashMap::new();
        while let Some(line_str) = lines.peek() {
            // let line_str = *line;
            let current_indent = line_str.len() - line_str.trim_start().len();
            let trimmed = line_str.trim();

            if trimmed.is_empty() || trimmed.starts_with("#") {
                lines.next();
                continue;
            }

            if current_indent < required_indent {
                break;
            }

            let line_to_process = lines.next().unwrap().trim();
            if let Some((code_str, path_str)) = line_to_process.split_once(':')
                && let Ok(code) = code_str.trim().parse::<u16>()
            {
                map.insert(code, Self::clean_val(path_str));
            }
        }
        map
    }

    fn parse_routes_map(
        lines: &mut std::iter::Peekable<std::str::Lines>,
    ) -> HashMap<String, RouteConfig> {
        let mut routes = HashMap::new();
        let mut current_path = String::new();
        let mut current_route = RouteConfig::default();

        while let Some(line) = lines.peek() {
            let indent = line.len() - line.trim_start().len();
            let trimmed = line.trim();

            if indent <= 2 && !trimmed.is_empty() {
                break;
            }

            let line_data = lines.next().unwrap().trim();
            if line_data.starts_with("- path:") {
                if !current_path.is_empty() {
                    routes.insert(current_path.clone(), current_route);
                }
                let path_val = line_data.trim_start_matches("- path:").trim();
                current_path = Self::clean_val(path_val);
                current_route = RouteConfig::default();
            } else if let Some((key, val)) = line_data.split_once(':') {
                let val = val.trim();
                match key.trim() {
                    "methods" => {
                        current_route.methods = val
                            .trim_matches(|c| c == '[' || c == ']')
                            .split([',', ' '])
                            .map(Self::clean_val)
                            .filter(|s| !s.is_empty())
                            .collect()
                    }
                    "root" => current_route.root = Self::clean_val(val),
                    "default_file" => current_route.default_file = Self::clean_val(val),
                    "autoindex" => current_route.autoindex = val == "true",
                    "cgi_ext" => current_route.cgi_ext = Some(Self::clean_val(val)),
                    "redirection" => current_route.redirection = Some(Self::clean_val(val)),
                    _ => {}
                }
            }
        }

        if !current_path.is_empty() {
            routes.insert(current_path, current_route);
        }

        routes
    }

    pub fn display_config(&self) {
        // Clear screen (optional, but professional)
        // print!("\x1b[2J\x1b[1;1H");

        println!("\n\x1b[1;35m 🌐 01_server CONFIGURATION DASHBOARD\x1b[0m");
        println!(
            "\x1b[38;5;240m ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m"
        );

        for (i, server) in self.servers.iter().enumerate() {
            let server_label = format!("SERVER BLOCK {:02}", i + 1);
            println!("\n  \x1b[1;37m{}\x1b[0m", server_label);
            println!("  \x1b[38;5;244m─────────────────────────────────────────\x1b[0m");

            // Info Grid
            println!(
                "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mNetwork:\x1b[0m    \x1b[32m{}\x1b[0m \x1b[38;5;244mvia ports\x1b[0m \x1b[1;32m{:?}\x1b[0m",
                server.host, server.ports
            );
            println!(
                "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mIdentitie:\x1b[0m  \x1b[36m{}\x1b[0m",
                server.server_name
            );
            println!(
                "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mLimits:\x1b[0m     \x1b[33m{} bytes\x1b[0m \x1b[38;5;244m(Max Body)\x1b[0m",
                server.client_max_body_size
            );

            println!("\n  \x1b[1;37mRouting Table:\x1b[0m");

            // Collect routes and sort them for a stable display
            let mut sorted_routes: Vec<_> = server.routes.iter().collect();
            sorted_routes.sort_by(|a, b| a.0.cmp(b.0));

            for (idx, (path, route)) in sorted_routes.iter().enumerate() {
                let is_last = idx == sorted_routes.len() - 1;
                let branch = if is_last {
                    "  └──"
                } else {
                    "  ├──"
                };
                let methods_fmt = route.methods.join("|");

                // Using ANSI background for methods makes them pop
                println!(
                    "  \x1b[38;5;244m{}\x1b[0m \x1b[1;37m{:12}\x1b[0m \x1b[48;5;236m\x1b[38;5;250m {} \x1b[0m ➔ \x1b[38;5;244mroot:\x1b[0m \x1b[3m{}\x1b[0m",
                    branch, path, methods_fmt, route.root
                );

                if let Some(cgi) = &route.cgi_ext {
                    let cgi_branch = if is_last { "     " } else { "  │  " };
                    println!(
                        "  \x1b[38;5;244m{}  └─ \x1b[0m\x1b[38;5;208mCGI Enabled: {}\x1b[0m",
                        cgi_branch, cgi
                    );
                }
            }
        }
        println!(
            "\n\x1b[38;5;240m ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m"
        );
        println!(" \x1b[1;32m✔\x1b[0m Server initialized and ready for events.\n");
    }
}
