use server_proxy::http::{HttpRequest, Method, ParsingState, ParseError};

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
fn test_query_parameter_parsing() {
    let mut req = HttpRequest::new();
    let raw = b"GET /search?query=rust&mode=fast HTTP/1.1\r\nHost: localhost\r\n\r\n";
    req.buffer.extend_from_slice(raw);

    let _ = req.parse_request();

    assert_eq!(req.url, "/search");
    assert_eq!(req.query_params.get("query").unwrap(), "rust");
    assert_eq!(req.query_params.get("mode").unwrap(), "fast");
}

#[test]
fn test_fragmented_headers() {
    let mut req = HttpRequest::new();
    
    // Chunk 1: Incomplete Request Line
    req.buffer.extend_from_slice(b"GET /path ");
    assert_eq!(req.parse_request().unwrap_err(), ParseError::IncompleteRequestLine);
    
    // Chunk 2: Complete Request Line, but no Headers
    req.buffer.extend_from_slice(b"HTTP/1.1\r\n");
    assert_eq!(req.parse_request().unwrap_err(), ParseError::IncompleteRequestLine);
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
    assert_eq!(req.parse_request().unwrap_err(), ParseError::IncompleteRequestLine);

    // Add remaining 5 bytes
    req.buffer.extend_from_slice(b"67890");
    assert!(req.parse_request().is_ok());
    assert_eq!(req.body, b"1234567890");
    assert_eq!(req.state, ParsingState::Complete);
}

#[test]
fn test_invalid_method() {
    let mut req = HttpRequest::new();
    req.buffer.extend_from_slice(b"PATCH /invalid HTTP/1.1\r\n\r\n");
    let result = req.parse_request();
    assert_eq!(result.unwrap_err(), ParseError::InvalidMethod);
}