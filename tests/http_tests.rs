use server_proxy::http::*;

#[test]
fn test_simple_get_request() {
    let mut req = HttpRequest::new();
    let raw = b"GET /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n";
    req.buffer.extend_from_slice(raw);

    let result = req.parse_request();

    assert!(result.is_ok());
    assert_eq!(req.method, Method::GET);
    assert_eq!(req.url, "/index.html");
    assert_eq!(req.state, ParsingState::Complete);
    assert_eq!(req.headers.get("Host").unwrap(), "localhost");
}

#[test]
fn test_fragmented_headers() {
    let mut req = HttpRequest::new();

    // Chunk 1: Incomplete Request Line
    req.buffer.extend_from_slice(b"GET /path ");
    assert_eq!(
        req.parse_request().unwrap_err(),
        ParseError::IncompleteRequestLine
    );

    // Chunk 2: Complete Request Line, but no Headers
    req.buffer.extend_from_slice(b"HTTP/1.1\r\n");
    assert_eq!(
        req.parse_request().unwrap_err(),
        ParseError::IncompleteRequestLine
    );
    assert_eq!(req.state, ParsingState::Headers);

    // Chunk 3: Complete Headers
    req.buffer.extend_from_slice(b"User-Agent: test\r\n\r\n");
    assert!(req.parse_request().is_ok());
    assert_eq!(req.state, ParsingState::Complete);
}

#[test]
fn test_post_request_with_body() {
    let mut req = HttpRequest::new();
    let raw = b"POST /api HTTP/1.1\r\nContent-Length: 13\r\n\r\nHello, World!";
    req.buffer.extend_from_slice(raw);

    let result = req.parse_request();

    assert!(result.is_ok());
    assert_eq!(req.method, Method::POST);
    assert_eq!(req.body, b"Hello, World!");
    assert_eq!(req.state, ParsingState::Complete);
}

#[test]
fn test_post_fragmented_body() {
    let mut req = HttpRequest::new();
    let head = b"POST /data HTTP/1.1\r\nContent-Length: 10\r\n\r\n";
    req.buffer.extend_from_slice(head);

    // Should transition to Body state but return Incomplete because 0/10 bytes read
    let res1 = req.parse_request();
    assert_eq!(res1.unwrap_err(), ParseError::IncompleteRequestLine);

    if let ParsingState::Body(len) = req.state {
        assert_eq!(len, 10);
    } else {
        panic!("Should be in Body state");
    }

    // Add 5 bytes
    req.buffer.extend_from_slice(b"12345");
    assert_eq!(
        req.parse_request().unwrap_err(),
        ParseError::IncompleteRequestLine
    );

    // Add remaining 5 bytes
    req.buffer.extend_from_slice(b"67890");
    assert!(req.parse_request().is_ok());
    assert_eq!(req.body, b"1234567890");
    assert_eq!(req.state, ParsingState::Complete);
}

#[test]
fn test_invalid_method() {
    let mut req = HttpRequest::new();
    req.buffer
        .extend_from_slice(b"PATCH /invalid HTTP/1.1\r\n\r\n");
    let result = req.parse_request();
    assert_eq!(result.unwrap_err(), ParseError::InvalidMethod);
}

#[test]
fn test_partial_request_parsing() {
    let mut req = HttpRequest::new();

    // Step 1: Send only the request line
    req.buffer
        .extend_from_slice(b"GET /index.html HTTP/1.1\r\n");
    let _ = req.parse_request();
    assert_eq!(req.state, ParsingState::Headers);
    assert_eq!(req.url, "/index.html");

    // Step 2: Send one header
    req.buffer.extend_from_slice(b"Host: localhost\r\n\r\n");
    let _ = req.parse_request();
    assert_eq!(req.state, ParsingState::Complete);
    assert_eq!(req.headers.get("Host").unwrap(), "localhost");
}

#[test]
fn test_body_parsing_logic() {
    let mut req = HttpRequest::new();
    let raw_request = b"POST /upload HTTP/1.1\r\nContent-Length: 5\r\n\r\nHelloWorld";

    req.buffer.extend_from_slice(raw_request);
    let _ = req.parse_request();

    // The state should be complete
    assert_eq!(req.state, ParsingState::Complete);
    // The body should ONLY contain "Hello" (first 5 bytes)
    assert_eq!(req.body, b"Hello");
    // The cursor should be at the start of "World" (remaining data)
    assert_eq!(req.cursor, raw_request.len() - 5);
}

#[test]
fn test_response_generation() {
    let res = HttpResponse::new(200, "OK")
        .set_header("Content-Type", "text/plain")
        .set_body(b"Hello Rust".to_vec(), "text/plain");

    let bytes = res.to_bytes();
    let s = String::from_utf8_lossy(&bytes);

    // Verify critical HTTP components
    assert!(s.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(s.contains("Content-Type: text/plain\r\n"));
    assert!(s.contains("Content-Length: 10\r\n"));
    assert!(s.ends_with("\r\n\r\nHello Rust"));
}


// #[test]
// fn test_security_traversal() {
//     let mut req = HttpRequest::new();
//     // Malicious URL attempting to escape the web root
//     req.url = "/../../etc/passwd".to_string();
    
//     let r_cfg = std::sync::Arc::new(RouteConfig {
//         root: "./public".to_string(),
//         ..Default::default()
//     });

//     // This should trigger your security check in handle_static_file
//     let response = Server::handle_static_file(&req, r_cfg);
//     assert_eq!(response.status_code, 403); // Forbidden
// }