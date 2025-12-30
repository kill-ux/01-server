use std::collections::HashMap;

use parser_derive::YamlStruct;

pub const DEFAULT_CLIENT_MAX_BODY_SIZE: usize = 1024 * 1024; // 1MB

#[derive(Debug, Clone, YamlStruct)]
pub struct RouteConfig {
    pub path: String,
    #[field(default = "[GET]")]
    pub methods: Vec<String>,
    pub redirection: Option<String>,
    pub root: String,
    #[field(default = "index.html")]
    pub default_file: String,
    pub cgi_ext: Option<String>,
    pub autoindex: Option<bool>,
}

#[derive(Debug, Clone, YamlStruct)]
pub struct ServerConfig {
    #[field(default = "127.0.0.1")]
    pub host: String,
    #[field(default = "[8080]")]
    pub ports: Vec<u16>,
    #[field(default = "_")]
    pub server_name: String,
    #[field(default = "{}")]
    pub error_pages: HashMap<u16, String>,
    #[field(default = "1048576")]
    pub client_max_body_size: usize,
    #[field(default = "[]")]
    pub routes: Vec<RouteConfig>,
    #[field(default = "false")]
    pub default_server: bool,
}

#[derive(Debug, Default, YamlStruct)]
pub struct AppConfig {
    pub servers: Vec<ServerConfig>,
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

            if server.client_max_body_size >= 1024 * 1024 {
                println!(
                    "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mBody Limit:\x1b[0m  \x1b[33m{} MB\x1b[0m",
                    server.client_max_body_size / (1024 * 1024)
                );
            } else {
                println!(
                    "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mBody Limit:\x1b[0m  \x1b[33m{} KB\x1b[0m",
                    server.client_max_body_size / 1024
                );
            }

            println!("  \x1b[1;34m⦿\x1b[0m \x1b[1;37mError Pages:\x1b[0m");
            for (code, path) in &server.error_pages {
                println!(
                    "    \x1b[38;5;244m{:4}\x1b[0m → \x1b[31m{}\x1b[0m",
                    code, path
                );
            }
            if server.error_pages.is_empty() {
                println!("    \x1b[38;5;244mNo custom error pages configured.\x1b[0m");
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
                let branch = if is_last {
                    "  └──"
                } else {
                    "  ├──"
                };
                let vertical_line = if is_last { "     " } else { "  │  " };

                let methods_fmt = route.methods.join(" | ");

                // 1. Path level
                println!(
                    "  \x1b[38;5;244m{}\x1b[0m \x1b[1;37m{}\x1b[0m",
                    branch, route.path
                );

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
                println!(
                    "  \x1b[38;5;244m{} ├─ Default:\x1b[0m   \x1b[36m{}\x1b[0m",
                    vertical_line, &route.default_file
                );

                // 4. Autoindex Check
                println!(
                    "  \x1b[38;5;244m{} ├─ Autoindex:\x1b[0m \x1b[{}m{}\x1b[0m",
                    vertical_line,
                    if route.autoindex.is_some() {
                        "32"
                    } else {
                        "31"
                    },
                    if route.autoindex.is_some() {
                        "ON"
                    } else {
                        "OFF"
                    }
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
