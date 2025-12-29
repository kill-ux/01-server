use std::collections::HashMap;

use parser_derive::YamlStruct;

pub const DEFAULT_CLIENT_MAX_BODY_SIZE: usize = 1024 * 1024; // 1MB

#[derive(Debug, Clone, YamlStruct)]
pub struct RouteConfig {
    pub path: String,
    pub methods: Vec<String>,
    pub redirection: Option<String>,
    pub root: String,
    pub default_file: Option<String>,
    pub cgi_ext: Option<String>,
    pub autoindex: Option<bool>,
}

#[derive(Debug, Clone, YamlStruct)]
pub struct ServerConfig {
    pub host: String,
    pub ports: Vec<u16>,
    pub server_name: String,
    pub error_pages: Option<HashMap<u16, String>>,
    pub client_max_body_size: Option<usize>,
    pub routes: Vec<RouteConfig>,
    pub default_server: bool,
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
            error_pages: None,
            client_max_body_size: Some(1024 * 1024),
            routes: Vec::new(),
            default_server: false,
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
        }
    }
}

impl AppConfig {
    pub fn display_config(&self) {
    println!("\n\x1b[1;35m 🌐 SERVER CONFIGURATION DASHBOARD\x1b[0m");
    println!(
        "\x1b[38;5;240m ════════════════════════════════════════════════════════════════\x1b[0m"
    );

    for (i, server) in self.servers.iter().enumerate() {
        let server_label = format!("SERVER BLOCK {:02}", i + 1);
        println!("\n  \x1b[1;37m{}\x1b[0m", server_label);
        println!("  \x1b[38;5;244m───────────────────────────────────────────────\x1b[0m");

        // Server Info Grid
        println!(
            "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mNetwork:\x1b[0m     \x1b[32m{}\x1b[0m \x1b[38;5;244mvia ports\x1b[0m \x1b[1;32m{:?}\x1b[0m",
            server.host, server.ports
        );
        println!(
            "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mIdentity:\x1b[0m    \x1b[36m{}\x1b[0m",
            server.server_name
        );
        println!(
            "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mDefault:\x1b[0m     \x1b[{}m{}\x1b[0m",
            if server.default_server { "32" } else { "31" },
            if server.default_server { "YES" } else { "NO" }
        );

        let server_limit = if let Some(n) = server.client_max_body_size {
            format!("{} KB", n / 1024)
        } else {
            // Ensure DEFAULT_CLIENT_MAX_BODY_SIZE is defined in your scope
            format!("{} KB", 1048576 / 1024) 
        };

        println!(
            "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mBody Limit:\x1b[0m  \x1b[33m{}\x1b[0m",
            server_limit
        );

        // Error Pages
        if let Some(error_pages) = &server.error_pages {
            println!("  \x1b[1;34m⦿\x1b[0m \x1b[1;37mError Pages:\x1b[0m");
            for (code, path) in error_pages {
                println!(
                    "    \x1b[38;5;244m{:4}\x1b[0m → \x1b[31m{}\x1b[0m",
                    code, path
                );
            }
        }

        // Routes Section
        println!(
            "\n  \x1b[1;37m📋 ROUTING TABLE ({}) \x1b[0m",
            server.routes.len()
        );
        println!("  \x1b[38;5;244m───────────────────────────────────────────────\x1b[0m");

        let mut sorted_routes: Vec<_> = server.routes.iter().collect();
        sorted_routes.sort_by(|a, b| a.path.cmp(&b.path));

        for (idx, route) in sorted_routes.iter().enumerate() {
            let is_last = idx == sorted_routes.len() - 1;
            let branch = if is_last { "  └──" } else { "  ├──" };
            let vertical_line = if is_last { "     " } else { "  │  " };
            
            let methods_fmt = route.methods.join(" | ");

            // 1. Path level
            println!("  \x1b[38;5;244m{}\x1b[0m \x1b[1;37m{}\x1b[0m", branch, route.path);

            // 2. Methods & Root (aligned under path)
            println!(
                "  \x1b[38;5;244m{} ├─ Methods:\x1b[0m   \x1b[48;5;236m\x1b[38;5;250m {} \x1b[0m",
                vertical_line, methods_fmt
            );
            println!(
                "  \x1b[38;5;244m{} ├─ Root:\x1b[0m      \x1b[32m{}\x1b[0m",
                vertical_line, route.root
            );

            // 3. Default File Check
            if let Some(default_file) = &route.default_file {
                println!(
                    "  \x1b[38;5;244m{} ├─ Default:\x1b[0m   \x1b[36m{}\x1b[0m",
                    vertical_line, default_file
                );
            } else {
                println!(
                    "  \x1b[38;5;244m{} ├─ Default:\x1b[0m   \x1b[31mNONE SET\x1b[0m",
                    vertical_line
                );
            }

            // 4. Autoindex Check
            println!(
                "  \x1b[38;5;244m{} ├─ Autoindex:\x1b[0m \x1b[{}m{}\x1b[0m",
                vertical_line,
                if route.autoindex.is_some() { "32" } else { "31" },
                if route.autoindex.is_some() { "ON" } else { "OFF" }
            );

            // 5. Redirection Check
            if let Some(redir) = &route.redirection {
                println!(
                    "  \x1b[38;5;244m{} ├─ Redirect:\x1b[0m  \x1b[35m{}\x1b[0m",
                    vertical_line, redir
                );
            }

            // 6. CGI Check (Closing branch of the route)
            if let Some(cgi) = &route.cgi_ext {
                println!(
                    "  \x1b[38;5;244m{} └─ CGI:\x1b[0m       \x1b[38;5;208m{}\x1b[0m",
                    vertical_line, cgi
                );
            } else {
                println!(
                    "  \x1b[38;5;244m{} └─ CGI:\x1b[0m       \x1b[31mDISABLED\x1b[0m",
                    vertical_line
                );
            }

            // Optional vertical separator between routes
            if !is_last {
                println!("  \x1b[38;5;244m  │\x1b[0m");
            }
        }
    }

    println!(
        "\n\x1b[38;5;240m ════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!(" \x1b[1;32m✔\x1b[0m Configuration loaded successfully - Ready for requests!\n");
}
}
