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
