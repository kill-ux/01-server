use std::fmt;
use crate::config::types::{ServerConfig, RouteConfig};

impl fmt::Display for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f, 
            "  \x1b[38;5;244m───────────────────────────────────────────────\x1b[0m"
        )?;
        writeln!(f, "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mNetwork:\x1b[0m     \x1b[32m{}\x1b[0m \x1b[38;5;244mvia ports\x1b[0m \x1b[1;32m{:?}\x1b[0m", self.host, self.ports)?;
        writeln!(
            f, 
            "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mIdentity:\x1b[0m    \x1b[36m{}\x1b[0m",
            self.server_name
        )?;
        writeln!(
            f, 
            "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mDefault:\x1b[0m     \x1b[{}m{}\x1b[0m",
            if self.default_server { "32" } else { "31" },
            if self.default_server { "YES" } else { "NO" }
        )?;
        writeln!(
            f, 
            "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mBody Limit:\x1b[0m  \x1b[33m{} KB\x1b[0m",
            self.client_max_body_size / 1024
        )?;

        if !self.error_pages.is_empty() {
            writeln!(f, "  \x1b[1;34m⦿\x1b[0m \x1b[1;37mError Pages:\x1b[0m")?;
            for (code, path) in &self.error_pages {
                writeln!(
                    f, 
                    "    \x1b[38;5;244m{:4}\x1b[0m → \x1b[31m{}\x1b[0m",
                    code, path
                )?;
            }
        }

        writeln!(
            f, 
            "\n  \x1b[1;37m📋 ROUTING TABLE ({}) \x1b[0m",
            self.routes.len()
        )?;
        writeln!(
            f, 
            "  \x1b[38;5;244m───────────────────────────────────────────────\x1b[0m"
        )?;

        let mut sorted_routes = self.routes.clone();
        sorted_routes.sort_by(|a, b| a.path.cmp(&b.path));

        for (idx, route) in sorted_routes.iter().enumerate() {
            let is_last = idx == sorted_routes.len() - 1;
            let branch = if is_last {
                "  └──"
            } else {
                "  ├──"
            };
            writeln!(
                f, 
                "  \x1b[38;5;244m{}\x1b[0m \x1b[1;37m{}\x1b[0m",
                branch, route.path
            )?;
            route.fmt_details(f, is_last)?;
            if !is_last {
                writeln!(f, "  \x1b[38;5;244m    │\x1b[0m")?;
            }
        }
        Ok(())
    }
}

impl RouteConfig {
    pub(crate) fn fmt_details(&self, f: &mut fmt::Formatter<'_>, is_last_route: bool) -> fmt::Result {
        let indent = if is_last_route { "     " } else { "  │  " };
        let methods_fmt = self.methods.join(" | ");

        writeln!(
            f, 
            "  \x1b[38;5;250m{}├─ Methods:\x1b[0m \x1b[48;5;236m\x1b[38;5;250m {}\x1b[0m",
            if is_last_route { "   " } else { "    " },
            methods_fmt
        )?;
        writeln!(
            f, 
            "  \x1b[38;5;250m{}├─ Root:\x1b[0m    \x1b[32m{}\x1b[0m",
            indent, self.root
        )?;
        writeln!(
            f, 
            "  \x1b[38;5;250m{}├─ Default:\x1b[0m  \x1b[36m{}\x1b[0m",
            indent, self.default_file
        )?;
        writeln!(
            f, 
            "  \x1b[38;5;250m{}├─ Autoindex:\x1b[0m \x1b[{}m{}\x1b[0m",
            indent,
            if self.autoindex { "32" } else { "31" },
            if self.autoindex { "ON" } else { "OFF" }
        )?;

        if let Some(redir) = &self.redirection {
            writeln!(
                f, 
                "  \x1b[38;5;250m{}├─ Redirect:\x1b[0m \x1b[35m{}\x1b[0m",
                indent, redir
            )?;
        }
        if let Some(cgi) = &self.cgi_ext {
            writeln!(
                f, 
                "  \x1b[38;5;250m{}└─ CGI:\x1b[0m     \x1b[38;5;208m{}\x1b[0m",
                indent, cgi
            )?;
        } else {
            writeln!(
                f, 
                "  \x1b[38;5;250m{}└─ CGI:\x1b[0m      \x1b[31mDISABLED\x1b[0m",
                indent
            )?;
        }
        Ok(())
    }
}

pub fn display_config(configs: &Vec<ServerConfig>) {
    println!("\n\x1b[1;35m 🌐 SERVER CONFIGURATION DASHBOARD\x1b[0m");
    println!(
        "\x1b[38;5;240m ════════════════════════════════════════════════════════════════\x1b[0m"
    );
    for (i, server) in configs.iter().enumerate() {
        println!("\n  \x1b[1;37mSERVER BLOCK {:02}\x1b[0m", i + 1);
        print!("{}", server);
    }
    println!(
        "\n\x1b[38;5;240m ════════════════════════════════════════════════════════════════\x1b[0m"
    );
    println!(" \x1b[1;32m✔\x1b[0m Configuration loaded successfully - Ready for requests!\n");
}
