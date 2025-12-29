use std::collections::HashMap;

use parser_derive::YamlStruct;


#[derive(Debug, Clone, YamlStruct)]
pub struct RouteConfig {
    pub path: String,
    pub methods: Vec<String>,
    pub redirection: Option<String>,
    pub root: String,
    pub default_file: Option<String>,
    pub cgi_ext: Option<String>,
    pub autoindex: Option<bool>,
    pub client_max_body_size: Option<usize>,
}


#[derive(Debug, Clone, YamlStruct)]
pub struct ServerConfig {
    pub host: String,
    pub ports: Vec<u16>,
    pub server_name: String,
    pub error_pages: HashMap<u16, String>,
    pub client_max_body_size: usize,
    pub routes: Vec<RouteConfig>,
}

#[derive(Debug, Default, YamlStruct)]
pub struct AppConfig {
    pub servers: Vec<ServerConfig>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            ports: Vec::new(),
            server_name: String::new(),
            error_pages: HashMap::new(),
            client_max_body_size: 1024 * 1024, // 1MB default
            routes: Vec::new(),
        }
    }
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            methods: vec!["GET".to_string()],
            path: "/".to_string(),
            redirection: None,
            root: String::new(),
            default_file: None,
            cgi_ext: None,
            autoindex: None,
            client_max_body_size: Some(1024 * 1024),
        }
    }
}

impl AppConfig {

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
            sorted_routes.sort_by(|a, b| a.path.cmp(&b.path));

            for (idx,  route) in sorted_routes.iter().enumerate() {
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
                    branch, route.path, methods_fmt, route.root
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
