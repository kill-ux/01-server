use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr},
};

use parser_derive::YamlStruct;
use proxy_log::{errors, warn};

use crate::{error::CleanError, http::Method, router::RoutingError};

pub const DEFAULT_CLIENT_MAX_BODY_SIZE: usize = 1024 * 1024; // 1MB
pub const ALLOWED_REDIRECTION_CODE: [u16; 5] = [301, 302, 303, 307, 308];

#[derive(Debug, Clone, YamlStruct)]
pub struct RouteConfig {
    pub path: String,
    #[parcast(default = "[GET]")]
    pub methods: Vec<String>,
    pub redirection: Option<String>,
    pub redirect_code: Option<u16>,
    #[parcast(default = "./www")]
    pub root: String,
    #[parcast(default = "index.html")]
    pub default_file: String,
    pub cgi_ext: Option<String>,
    #[parcast(default = "false")]
    pub autoindex: bool,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            methods: vec!["GET".into()],
            root: "./www".into(),
            default_file: "index.html".into(),
            autoindex: false,
            redirection: None,
            redirect_code: None,
            cgi_ext: None,
        }
    }
}

#[derive(Debug, Clone, YamlStruct)]
pub struct ServerConfig {
    #[parcast(default = "127.0.0.1", rename = "host")]
    pub host_str: String,
    #[parcast(skip)]
    pub host: IpAddr,
    #[parcast(default = "[8080]")]
    pub ports: Vec<u16>,
    #[parcast(default = "_")]
    pub server_name: String,
    #[parcast(default = "{}")]
    pub error_pages: HashMap<u16, String>,
    #[parcast(default = "1048576")]
    pub client_max_body_size: usize,
    #[parcast(default = "[]")]
    pub routes: Vec<RouteConfig>,
    #[parcast(default = "false")]
    pub default_server: bool,
    #[parcast(default = "./www")]
    pub root: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            host_str: "127.0.0.1".to_string(),
            ports: vec![8080],
            server_name: "_".to_string(),
            error_pages: HashMap::new(),
            client_max_body_size: 1048576,
            routes: Vec::new(),
            default_server: false,
            root: "./www".to_string(),
        }
    }
}

impl ServerConfig {
    pub fn host_header(&self) -> String {
        match self.host {
            IpAddr::V4(ip) => ip.to_string(),
            IpAddr::V6(ip) => format!("[{ip}]"),
        }
    }

    pub fn find_route(&self, path: &str, method: &Method) -> Result<&RouteConfig, RoutingError> {
        let mut best_match: Option<(&String, &RouteConfig)> = None;
        for route in &self.routes {
            if path.starts_with(&route.path) {
                match best_match {
                    None => best_match = Some((&route.path, route)),
                    Some((best_prefix, _)) => {
                        if route.path.len() > best_prefix.len() {
                            best_match = Some((&route.path, route));
                        }
                    }
                }
            }
        }

        if let Some((_, r_cfg)) = best_match {
            if method.is_allowed(&r_cfg.methods) {
                return Ok(r_cfg);
            } else {
                return Err(RoutingError::MethodNotAllowed);
            }
        }

        Err(RoutingError::NotFound)
    }
}

#[derive(Debug, Default, YamlStruct)]
pub struct AppConfig {
    pub servers: Vec<ServerConfig>,
}

impl AppConfig {
    pub fn validate(&mut self) -> Result<(), CleanError> {
        let mut virtual_hosts = HashSet::new();
        let mut default_servers_per_port = HashMap::new();
        let mut valid_servers = Vec::new();

        // Use drain to take ownership and rebuild the list
        for mut s_cfg in self.servers.drain(..) {
            let mut is_valid = true;
            let mut local_ports = HashSet::new();

            let s_root = std::path::Path::new(&s_cfg.root);
            if !s_root.exists() || !s_root.is_dir() {
                errors!(
                    "Server '{}': Global root '{:?}' is invalid.",
                    s_cfg.server_name,
                    s_cfg.root
                );
                is_valid = false;
            }

            for (&code, path_str) in &s_cfg.error_pages {
                let full_path =
                    std::path::Path::new(&s_cfg.root).join(path_str.trim_start_matches('/'));
                if !full_path.exists() || !full_path.is_file() {
                    warn!(
                        "Server '{}': Error page {} for code {} not found or is not a file.",
                        s_cfg.server_name,
                        full_path.display(),
                        code
                    );
                    // We don't necessarily kill the server for a missing error page,
                    // but we warn the user.
                }
            }

            // 1. IP Validation
            if sync_host_fields(&mut s_cfg).is_err() {
                errors!("Invalid IP address format: {}", s_cfg.host);
                is_valid = false;
            }

            // 2. Default Server uniqueness per port
            for &port in &s_cfg.ports {
                if port == 0 {
                    errors!(
                        "Server '{}' requested port 0. Static port assignment is required.",
                        s_cfg.server_name
                    );
                    is_valid = false;
                    break;
                }

                if !local_ports.insert(port) {
                    warn!("Server {} has duplicated ports.", s_cfg.server_name);
                    is_valid = false;
                    break;
                }

                if s_cfg.default_server {
                    if let Some(existing_name) =
                        default_servers_per_port.insert(port, s_cfg.server_name.clone())
                    {
                        return Err(CleanError::from(format!(
                            "Conflict: Multiple default servers on port {}. Found '{}' and '{}'",
                            port, existing_name, s_cfg.server_name
                        )));
                    }
                }

                // 3. Virtual Host check (Duplicate server_name on same port)
                let vhost_id = format!("{}:{}", s_cfg.server_name, port);
                if !virtual_hosts.insert(vhost_id.clone()) {
                    return Err(CleanError::from(format!(
                        "Duplicate virtual host: {}",
                        vhost_id
                    )));
                }
            }

            // 4. Route & Path Validation
            for route in &s_cfg.routes {
                // Check if root exists
                let path = std::path::Path::new(&route.root);
                if !path.exists() {
                    warn!(
                        "Server '{}' route '{}' root does not exist: {}",
                        s_cfg.server_name, route.path, route.root
                    );
                    is_valid = false;
                }

                // Check methods
                for method in &route.methods {
                    match method.parse::<Method>() {
                        Ok(_) => {}
                        _ => {
                            errors!(
                                "Invalid method '{}' found in config for server '{}'",
                                method,
                                s_cfg.server_name
                            );
                            is_valid = false;
                            break;
                        }
                    }
                }

                if let Some(code) = route.redirect_code {
                    if !ALLOWED_REDIRECTION_CODE.contains(&code) {
                        errors!(
                            "Invalid status code {} for redirection found in config for server '{}'",
                            code,
                            s_cfg.server_name
                        );
                        is_valid = false;
                    }
                }
            }

            if is_valid {
                valid_servers.push(s_cfg);
            } else {
                warn!("Server has misconfiguration: Skiped");
            }
        }

        if valid_servers.is_empty() {
            return Err("Zero valid server blocks found in configuration.".into());
        }
        self.servers = valid_servers;
        Ok(())
    }

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
                    if route.autoindex { "32" } else { "31" },
                    if route.autoindex { "ON" } else { "OFF" }
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

pub fn sync_host_fields(config: &mut ServerConfig) -> Result<(), CleanError> {
    let host_str = &mut config.host_str;

    let inner = if host_str.starts_with('[') && host_str.ends_with(']') {
        &host_str[1..host_str.len() - 1]
    } else {
        host_str.as_str()
    };

    let addr = inner
        .parse::<IpAddr>()
        .map_err(|_| CleanError::from(format!("Invalid IP: {}", host_str)))?;

    // Update the IpAddr field
    config.host = addr;

    // Standardize the String field for the host header
    if addr.is_ipv6() {
        *host_str = format!("[{}]", addr);
    } else {
        *host_str = addr.to_string();
    }

    Ok(())
}
