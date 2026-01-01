use parser::FromYaml;
use server_proxy::config::{AppConfig, ServerConfig};

#[cfg(test)]
mod tests {
    use server_proxy::error::CleanError;

    use super::*;

    fn err_to_str(e: CleanError) -> String {
        format!("{}", e)
    }

    #[test]
    fn test_valid_server_config() {
        let yaml_str = "
            host: 0.0.0.0
            ports: [80, 443]
            server_name: myserv
            client_max_body_size: 2048
            routes:
              - path: /
                root: ./www
        ";
        let config = ServerConfig::from_str(yaml_str).unwrap();

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.ports, vec![80, 443]);
        assert_eq!(config.server_name, "myserv");
        assert_eq!(config.client_max_body_size, 2048);
        assert_eq!(config.routes[0].path, "/");
    }

    #[test]
    fn test_valid_config_1() {
        let yaml_str = r#"
servers:
  - host: "127.0.0.1"
    ports: [8080, 8081]
    server_name: "test_server"
    default_server: true
    client_max_body_size: 1024
    routes:
      - path: "/"
        methods: ["GET"]
        root: "./www"
        default_file: "index.html"
        autoindex: true
"#;
        let config = AppConfig::from_str(yaml_str).expect("Should parse valid config");
        assert_eq!(config.servers.len(), 1);
        let server = &config.servers[0];
        assert_eq!(server.host, "127.0.0.1");
        assert_eq!(server.ports, vec![8080, 8081]);
        assert_eq!(server.server_name, "test_server");
        assert!(server.default_server);
        assert_eq!(server.client_max_body_size, 1024);
        assert_eq!(server.routes.len(), 1);
        assert_eq!(server.routes[0].path, "/");
    }

    #[test]
    fn test_missing_colon() {
        let yaml = r#"
servers:
  - host "127.0.0.1"
"#;
        let err = err_to_str((AppConfig::from_str(yaml).unwrap_err()).into());
        println!("{}", err);
        assert!(err.contains("Expected a Map") || err.contains("Expected"));
    }

    #[test]
    fn test_unknown_field() {
        let yaml = r#"
servers:
  - host: "127.0.0.1"
    unknown_field: "some_value"
    server_name: "test"
"#;
        let config = AppConfig::from_str(yaml).expect("Parses partially");
        assert_eq!(config.servers[0].host, "127.0.0.1");
        assert_eq!(config.servers[0].server_name, "test");
    }

    #[test]
    fn test_wrong_indentation() {
        let yaml_bad = r#"
servers:
  - host: "127.0.0.1"
    ports: [8080]
   server_name: "bad_indent"
"#;
        let err = err_to_str((AppConfig::from_str(yaml_bad).unwrap_err()).into());
        println!("{}", err);
        assert!(err.contains("Expected '-' for next item") || err.contains("Expected"));
    }

    #[test]
    fn test_type_mismatch() {
        let yaml = r#"
servers:
  - host: "127.0.0.1"
    client_max_body_size: "not a number"
"#;
        let err = err_to_str((AppConfig::from_str(yaml).unwrap_err()).into());
        println!("{}", err);
        assert!(err.contains("invalid digit found in string"));
    }

    #[test]
    fn test_list_parsing_error() {
        let yaml = r#"
servers:
  - host: "127.0.0.1"
    ports: [8080, "bad_port"]
"#;
        let err = err_to_str((AppConfig::from_str(yaml).unwrap_err()).into());
        println!("{}", err);
        assert!(err.contains("invalid digit found in string"));
    }

    #[test]
    fn test_full_app_config() {
        let yaml_str = "
        servers:
          - server_name: web1
            host: 127.0.0.1
            ports: [8080, 8081]
            routes:
              - path: /
                root: ./web1/www
          - server_name: web2
            host: 127.0.0.1
            ports: [9090]
            routes:
              - path: /
                root: ./web2/www
        ";
        let config = AppConfig::from_str(yaml_str).unwrap();
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers[0].server_name, "web1");
        assert_eq!(config.servers[1].ports, vec![9090]);
    }

    #[test]
    fn test_default_values() {
        let yaml_str = "server_name: test_default";
        let config = ServerConfig::from_str(yaml_str).unwrap();

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.ports, vec![8080]);
        assert_eq!(config.routes.len(), 0);
    }

    #[test]
    fn test_unknown_field_handling() {
        let yaml_str = "
        host: 127.0.0.1
        fake_setting: 123
    ";
        let config = ServerConfig::from_str(yaml_str);
        assert!(config.is_ok());
    }

    #[test]
    fn test_error_pages_default() {
        let yaml_str = "host: 127.0.0.1";
        let config = ServerConfig::from_str(yaml_str).unwrap();
        assert!(config.error_pages.is_empty());
    }

    #[test]
    fn test_invalid_port_type() {
        let yaml_str = "ports: [80, 'abc']";
        let result = ServerConfig::from_str(yaml_str);
        assert!(result.is_err());
        let err = err_to_str((result.unwrap_err()).into());
        assert!(
            err.contains("invalid digit found in string")
        );
    }

    #[test]
    fn test_invalid_client_max_body_size_type() {
        let yaml_str = "client_max_body_size: abc";
        let result = ServerConfig::from_str(yaml_str);
        assert!(result.is_err());
        let err = err_to_str((result.unwrap_err()).into());
        assert!(err.contains("invalid digit found in string"));
    }

    #[test]
    fn test_missing_required_path_in_route() {
        let yaml_str = "
        routes:
          - root: /tmp
    ";
        let result = ServerConfig::from_str(yaml_str);
        assert!(result.is_err());
        let err = err_to_str((result.unwrap_err()).into());
        assert!(err.contains("Missing required field: path"));
    }

    #[test]
    fn test_invalid_autoindex_type_in_route() {
        let yaml_str = "
        routes:
          - path: /
            autoindex: yes
    ";
        let result = ServerConfig::from_str(yaml_str);
        assert!(result.is_err());
        let err = err_to_str((result.unwrap_err()).into());
        assert!(err.contains("Invalid boolean"));
    }

    #[test]
    fn test_bad_syntax() {
        let yaml_str = "host: : 127.0.0.1";
        let result = ServerConfig::from_str(yaml_str);
        assert!(result.is_err());
    }
}
