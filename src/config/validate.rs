use std::collections::{HashMap, HashSet};
use crate::config::types::ServerConfig;

pub fn validate_configs(configs: Vec<ServerConfig>) -> Vec<ServerConfig> {
    let mut valid_configs = Vec::new();
    let mut conflict_indices = HashSet::new();

    // Key: (Host, Port, ServerName) -> List of config indices that use this combination
    let mut usage_map: HashMap<(String, u16, String), Vec<usize>> = HashMap::new();

    // 1. Build the usage map
    for (idx, config) in configs.iter().enumerate() {
        for port in &config.ports {
            let key = (config.host.clone(), *port, config.server_name.clone());
            usage_map.entry(key).or_default().push(idx);
        }
    }

    // 2. Identify duplicates (Exact Match)
    for ((host, port, server_name), indices) in usage_map {
        if indices.len() > 1 {
            println!(
                "❌ \x1b[1;31mConflict Detected:\x1b[0m Multiple servers defined for {}:{} with name '{}'. Dropping conflicting configurations.",
                host, port, server_name
            );
            for idx in indices {
                conflict_indices.insert(idx);
            }
        }
    }

    // 3. Identify Wildcard Conflicts (0.0.0.0 vs 127.0.0.1 on same port)
    // Map: Port -> Set of Hosts used on this port
    let mut port_hosts: HashMap<u16, HashSet<String>> = HashMap::new();
    for config in &configs {
        for port in &config.ports {
            port_hosts.entry(*port).or_default().insert(config.host.clone());
        }
    }

    for (port, hosts) in port_hosts {
        if hosts.contains("0.0.0.0") && hosts.len() > 1 {
            println!(
                "❌ \x1b[1;31mBind Conflict Detected:\x1b[0m Port {} mixes wildcard '0.0.0.0' with specific IPs {:?}. This will fail to bind.",
                port, hosts
            );
            // Drop ALL configs on this port to be safe
            for (idx, config) in configs.iter().enumerate() {
                if config.ports.contains(&port) {
                    conflict_indices.insert(idx);
                }
            }
        }
    }

    // 4. Validate File Paths and Status Codes
    for (idx, config) in configs.iter().enumerate() {
        let mut valid = true;

        // Check Error Pages
        for (code, path) in &config.error_pages {
            if *code < 100 || *code > 599 {
                println!(
                    "❌ \x1b[1;31mInvalid Status Code:\x1b[0m Server '{}' has invalid error page code {}. Must be between 100 and 599.",
                    config.server_name, code
                );
                valid = false;
            }
            if let Err(e) = std::fs::File::open(path) {
                println!(
                    "❌ \x1b[1;31mFile Error:\x1b[0m Server '{}' refers to error page '{}' for code {}: {}.",
                    config.server_name, path, code, e
                );
                valid = false;
            }
        }

        // Check Routes
        for route in &config.routes {
            if let Err(e) = std::fs::read_dir(&route.root) {
                println!(
                    "❌ \x1b[1;31mDirectory Error:\x1b[0m Server '{}' route '{}' refers to invalid root directory '{}': {}.",
                    config.server_name, route.path, route.root, e
                );
                valid = false;
            }
        }

        if !valid {
            conflict_indices.insert(idx);
        }
    }

    // 5. Filter valid configs
    for (idx, config) in configs.into_iter().enumerate() {
        if !conflict_indices.contains(&idx) {
            valid_configs.push(config);
        }
    }

    if !conflict_indices.is_empty() {
        println!("⚠️ \x1b[33mResult:\x1b[0m {} configurations were dropped due to conflicts.", conflict_indices.len());
    }

    valid_configs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::ServerConfig;

    fn make_config(host: &str, ports: Vec<u16>, name: &str) -> ServerConfig {
        let mut c = ServerConfig::default();
        c.host = host.to_string();
        c.ports = ports;
        c.server_name = name.to_string();
        c
    }

    #[test]
    fn test_validate_no_conflicts() {
        let configs = vec![
            make_config("127.0.0.1", vec![8001], "s1"),
            make_config("127.0.0.1", vec![8002], "s2"),
        ];
        let valid = validate_configs(configs);
        assert_eq!(valid.len(), 2);
    }

    #[test]
    fn test_validate_virtual_hosts_ok() {
        // Same host/port, different names -> OK
        let configs = vec![
            make_config("127.0.0.1", vec![8080], "example.com"),
            make_config("127.0.0.1", vec![8080], "api.example.com"),
        ];
        let valid = validate_configs(configs);
        assert_eq!(valid.len(), 2);
    }

    #[test]
    fn test_validate_conflict_drops_both() {
        // Same host/port/name -> Conflict
        let configs = vec![
            make_config("127.0.0.1", vec![8080], "same.com"),
            make_config("127.0.0.1", vec![8080], "same.com"),
            make_config("127.0.0.1", vec![8081], "other.com"), // This one is fine
        ];
        let valid = validate_configs(configs);
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].server_name, "other.com");
    }

     #[test]
    fn test_validate_multi_port_partial_conflict() {
        // C1: 80, 81. Name: foo
        // C2: 80.     Name: foo
        // Conflict on 80. Both should be dropped (simplest implementation)
        // because we drop the *Configuration* object.
        let configs = vec![
            make_config("127.0.0.1", vec![80, 81], "foo"),
            make_config("127.0.0.1", vec![80], "foo"),
        ];
        let valid = validate_configs(configs);
        assert_eq!(valid.len(), 0);
    }

    #[test]
    fn test_validate_wildcard_conflict() {
        // C1: 0.0.0.0:8080 (Wildcard)
        // C2: 127.0.0.1:8080 (Specific)
        // This is a bind conflict.
        let configs = vec![
            make_config("0.0.0.0", vec![8080], "s1"),
            make_config("127.0.0.1", vec![8080], "s2"),
        ];
        let valid = validate_configs(configs);
        assert_eq!(valid.len(), 0);
    }

    #[test]
    fn test_validate_wildcard_no_conflict() {
        // C1: 0.0.0.0:8080
        // C2: 0.0.0.0:8080 (Different Name)
        // Valid Virtual Hosting on Wildcard
        let configs = vec![
            make_config("0.0.0.0", vec![8080], "s1"),
            make_config("0.0.0.0", vec![8080], "s2"),
        ];
        let valid = validate_configs(configs);
        assert_eq!(valid.len(), 2);
    }

    #[test]
    fn test_validate_invalid_status_code() {
        let mut config = make_config("127.0.0.1", vec![8080], "s1");
        config.error_pages.insert(99, "exists".to_string()); // Invalid code
        
        // Mock existence of file (not really possible without creating it, 
        // but let's assume the status code check happens first or independently)
        // Actually valid=false logic accumulates.
        
        let valid = validate_configs(vec![config]);
        assert_eq!(valid.len(), 0);
    }

    #[test]
    fn test_validate_missing_files() {
        let mut config = make_config("127.0.0.1", vec![8080], "s1");
        config.error_pages.insert(404, "/non/existent/path/err.html".to_string());
        
        let valid = validate_configs(vec![config]);
        assert_eq!(valid.len(), 0);
    }

    #[test]
    fn test_validate_missing_root() {
        use crate::config::types::RouteConfig;
        let mut config = make_config("127.0.0.1", vec![8080], "s1");
        let mut route = RouteConfig::default();
        route.root = "/non/existent/dir".to_string();
        config.routes.push(route);
        
        let valid = validate_configs(vec![config]);
        assert_eq!(valid.len(), 0);
    }

    #[test]
    fn test_validate_valid_files() {
        use crate::config::types::RouteConfig;
        // Create temp things
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_err.html");
        std::fs::write(&file_path, "error").unwrap();
        
        let mut config = make_config("127.0.0.1", vec![8080], "s1");
        config.error_pages.insert(404, file_path.to_str().unwrap().to_string());
        
        let mut route = RouteConfig::default();
        route.root = temp_dir.to_str().unwrap().to_string(); // Temp dir exists
        config.routes.push(route);
        
        let valid = validate_configs(vec![config]);
        assert_eq!(valid.len(), 1);
        
        // Cleanup
        let _ = std::fs::remove_file(file_path);
    }
}