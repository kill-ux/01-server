use server_proxy::config::AppConfig;

#[test]
fn test_config_parsing() {
    let yaml = r#"
server:
  host: "127.0.0.1"
  ports: [8080, 9000]
  routes:
    - path: "/static"
      root: "./public"
      methods: ["GET", "POST"]
"#;
    let config = AppConfig::parse_str(yaml);
    let server = &config.servers[0];

    assert_eq!(server.host, "127.0.0.1"); // Check if quotes were removed
    assert_eq!(server.ports, vec![8080, 9000]);

    let route = server.routes.get("/static").unwrap();
    assert_eq!(route.root, "./public");
    assert!(route.methods.contains(&"GET".to_string()));
}

#[test]
fn test_quote_variants() {
    let yaml = r#"
server:
  host: "127.0.0.1"
  server_name: 'localhost proxy'
  routes:
    - path: /static
      root: "./public"
"#;
    let config = AppConfig::parse_str(yaml);
    let s = &config.servers[0];
    let r = s.routes.get("/static").unwrap();

    assert_eq!(s.host, "127.0.0.1"); // Double quotes removed
    assert_eq!(s.server_name, "localhost"); // Single quotes from list removed
    assert_eq!(r.root, "./public"); // Double quotes in route removed
}


#[test]
fn test_messy_formatting() {
    let yaml = r#"
server:
  host: 127.0.0.1

  # This is a comment in the middle
  ports: [8080]

  routes:
    - path: "/"
      root: ./html

"#; // Trailing newlines
    let config = AppConfig::parse_str(yaml);
    assert_eq!(config.servers.len(), 1);
    assert_eq!(config.servers[0].ports, vec![8080]);
    assert!(config.servers[0].routes.contains_key("/"));
}


#[test]
fn test_multiple_servers() {
    let yaml = r#"
server:
  host: 127.0.0.1
  ports: [80]
server:
  host: 127.0.0.2
  ports: [443]
"#;
    let config = AppConfig::parse_str(yaml);
    assert_eq!(config.servers.len(), 2);
    assert_eq!(config.servers[0].host, "127.0.0.1");
    assert_eq!(config.servers[1].host, "127.0.0.2");
}

#[test]
fn test_error_page_styles() {
    // Style 1: Inline
    let yaml_inline = "server:\n  error_pages: {404: /404.html, 500: /500.html}";
    let conf1 = AppConfig::parse_str(yaml_inline);
    assert_eq!(conf1.servers[0].error_pages.get(&404).unwrap(), "/404.html");

    // Style 2: Block
    let yaml_block = r#"
server:
  error_pages:
    404: /404.html
    500: /500.html
"#;
    let conf2 = AppConfig::parse_str(yaml_block);
    assert_eq!(conf2.servers[0].error_pages.get(&404).unwrap(), "/404.html");
}


#[test]
fn test_list_parsing() {
    let yaml = r#"
server:
  ports: [8080, 9000]
  server_names: "example.com web_server"
"#;
    let config = AppConfig::parse_str(yaml);
    let s = &config.servers[0];
    
    assert_eq!(s.ports, vec![8080, 9000]);
    assert_eq!(s.server_name, "example.com");
}