#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub fn parse_http_request(buf: &[u8]) -> Option<HttpRequest> {
    // Try to find the end of headers
    let headers_end = find_headers_end(buf)?;

    let headers_bytes = &buf[..headers_end];
    let body_bytes = &buf[headers_end..];

    // Convert headers to string
    let headers_str = String::from_utf8_lossy(headers_bytes);

    // Parse start line
    let mut lines = headers_str.lines();
    let start_line = lines.next()?;
    let mut start_parts = start_line.split_whitespace();
    let method = start_parts.next()?.to_string();
    let path = start_parts.next()?.to_string();
    let version = start_parts.next()?.to_string();

    // Parse header lines into key-value pairs
    let mut headers = Vec::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.push((key.trim().to_string(), value.trim().to_string()));
        }
    }

    // Determine content length
    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("Content-Length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);

    // Check if we have the full body yet
    if body_bytes.len() < content_length {
        return None; // Not enough bytes yet
    }

    let body = body_bytes[..content_length].to_vec();

    Some(HttpRequest {
        method,
        path,
        version,
        headers,
        body,
    })
}

// helpers
fn find_headers_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| pos + 4)
}

// // ============================================================================
// // http_processor.rs
// // ============================================================================

// use std::collections::HashMap;

// pub struct HttpProcessor {
//     data_provider: DataProvider,
// }

// impl HttpProcessor {
//     pub fn new(data_provider: DataProvider) -> Self {
//         Self { data_provider }
//     }

//     /// Process an HTTP request and generate a response
//     pub fn process_request(&self, request: &HttpRequest) -> HttpResponse {
//         match request.method.as_str() {
//             "GET" => self.handle_get(request),
//             "POST" => self.handle_post(request),
//             "HEAD" => self.handle_head(request),
//             _ => self.method_not_allowed(),
//         }
//     }

//     fn handle_get(&self, request: &HttpRequest) -> HttpResponse {
//         match self.data_provider.read_file(&request.path) {
//             Ok(content) => {
//                 let mime_type = self.data_provider.get_mime_type(&request.path);
//                 HttpResponse::ok(content, mime_type)
//             }
//             Err(_) => self.not_found(),
//         }
//     }

//     fn handle_post(&self, request: &HttpRequest) -> HttpResponse {
//         // Example: Echo back the POST body
//         let response_body = format!(
//             "Received POST to {} with {} bytes",
//             request.path,
//             request.body.len()
//         );
//         HttpResponse::ok(response_body.into_bytes(), "text/plain")
//     }

//     fn handle_head(&self, request: &HttpRequest) -> HttpResponse {
//         if self.data_provider.file_exists(&request.path) {
//             let mime_type = self.data_provider.get_mime_type(&request.path);
//             HttpResponse::ok(Vec::new(), mime_type) // Empty body for HEAD
//         } else {
//             self.not_found()
//         }
//     }

//     fn not_found(&self) -> HttpResponse {
//         HttpResponse {
//             status_code: 404,
//             status_text: "Not Found".to_string(),
//             headers: HashMap::new(),
//             body: b"<html><body><h1>404 Not Found</h1></body></html>".to_vec(),
//         }
//     }

//     fn method_not_allowed(&self) -> HttpResponse {
//         HttpResponse {
//             status_code: 405,
//             status_text: "Method Not Allowed".to_string(),
//             headers: HashMap::new(),
//             body: b"<html><body><h1>405 Method Not Allowed</h1></body></html>".to_vec(),
//         }
//     }
// }

// pub struct HttpResponse {
//     pub status_code: u16,
//     pub status_text: String,
//     pub headers: HashMap<String, String>,
//     pub body: Vec<u8>,
// }

// impl HttpResponse {
//     pub fn ok(body: Vec<u8>, content_type: &str) -> Self {
//         let mut headers = HashMap::new();
//         headers.insert("Content-Type".to_string(), content_type.to_string());
//         headers.insert("Content-Length".to_string(), body.len().to_string());

//         Self {
//             status_code: 200,
//             status_text: "OK".to_string(),
//             headers,
//             body,
//         }
//     }

//     /// Convert response to bytes for sending over the network
//     pub fn to_bytes(&self) -> Vec<u8> {
//         let mut response = format!(
//             "HTTP/1.1 {} {}\r\n",
//             self.status_code, self.status_text
//         );

//         for (key, value) in &self.headers {
//             response.push_str(&format!("{}: {}\r\n", key, value));
//         }

//         response.push_str("\r\n");

//         let mut bytes = response.into_bytes();
//         bytes.extend_from_slice(&self.body);
//         bytes
//     }
// }
