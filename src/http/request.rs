use std::{
    collections::HashMap,
    fmt::{self, Display},
    fs::File,
    io::Write,
    path::PathBuf,
    process::ChildStdin,
    str::FromStr,
    sync::Arc,
    time::SystemTime,
};

use crate::{
    http::HttpResponse,
    router::RoutingError,
    server::{
        ActiveAction, HTTP_FOUND, HTTP_METHOD_NOT_ALLOWED, HTTP_NOT_FOUND, HttpConnection, Server,
        Upload, UploadState,
    },
};

const _1MB: usize = 1_024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Method {
    GET,
    POST,
    DELETE,
}

impl Method {
    pub fn is_allowed(&self, allowed_methods: &Vec<String>) -> bool {
        allowed_methods.contains(&self.to_string())
    }

    pub fn as_str(&self) -> &str {
        match self {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::DELETE => "DELETE",
        }
    }
}

impl FromStr for Method {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(Method::GET),
            "POST" => Ok(Method::POST),
            "DELETE" => Ok(Method::DELETE),
            _ => Err(ParseError::InvalidMethod),
        }
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::DELETE => "DELETE",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, PartialEq)]
pub enum ParsingState {
    RequestLine,
    Headers,
    HeadersDone,
    Body(usize, usize),
    ChunkedBody(usize),
    Complete,
    Error,
}

const CRLN_LEN: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    IncompleteRequestLine,
    MalformedRequestLine,
    InvalidMethod,
    InvalidUtf8(std::string::FromUtf8Error),
    UnexpectedEof,
    HeaderTooLong,
    TooManyHeaders,
    InvalidHeaderName,
    InvalidHeaderValue,
    PayloadTooLarge,
    ParseHexError,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::IncompleteRequestLine => write!(f, "Incomplete request line"),
            ParseError::MalformedRequestLine => write!(f, "Malformed request line"),
            ParseError::InvalidMethod => write!(f, "Invalid HTTP method"),
            ParseError::InvalidUtf8(_) => write!(f, "Invalid UTF-8 in request"),
            ParseError::UnexpectedEof => write!(f, "Unexpected end of input"),
            ParseError::HeaderTooLong => write!(f, "Header line too long"),
            ParseError::TooManyHeaders => write!(f, "Too many headers"),
            ParseError::InvalidHeaderName => write!(f, "Invalid header name"),
            ParseError::InvalidHeaderValue => write!(f, "Invalid header value"),
            ParseError::PayloadTooLarge => write!(f, "Payload too large"),
            ParseError::ParseHexError => write!(f, "Parse Hex Error"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<std::string::FromUtf8Error> for ParseError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        ParseError::InvalidUtf8(err)
    }
}

#[derive(Debug)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub body_file: Option<File>,
    pub is_large_body: bool,
    pub buffer: Vec<u8>,
    pub cursor: usize,
    pub state: ParsingState,
}

impl Default for HttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpRequest {
    pub fn new() -> Self {
        HttpRequest {
            method: Method::GET,
            url: String::new(),
            version: String::new(),
            headers: HashMap::new(),
            body: Vec::new(),
            buffer: Vec::with_capacity(4096),
            cursor: 0,
            state: ParsingState::RequestLine,
            is_large_body: false,
            body_file: None,
        }
    }

    pub fn clear(&mut self) {
        self.state = ParsingState::RequestLine;
        self.headers.clear();
        self.body.clear();
    }

    pub fn finish_request(&mut self) {
        self.buffer.drain(..self.cursor);
        self.cursor = 0;
        self.clear();
    }

    pub fn parse_request(conn: &mut HttpConnection) -> core::result::Result<(), ParseError> {
        loop {
            let res = match conn.request.state {
                ParsingState::RequestLine => conn.request.parse_request_line(),
                ParsingState::Headers => HttpRequest::parse_headers(conn),
                ParsingState::HeadersDone => {
                    if let Some(res) = HttpRequest::setup_action(conn) {
                        ///// dddddddddd
                        conn.write_buffer.extend_from_slice(&res.to_bytes());
                        conn.request.state = ParsingState::Complete;
                    }
                    Ok(())
                }
                ParsingState::Body(cl, max) => HttpRequest::parse_unchunked_body(conn, cl),
                ParsingState::ChunkedBody(max) => match conn.request.parse_chunked_body(max) {
                    Ok(true) => {
                        conn.request.state = ParsingState::Complete;
                        Ok(())
                    }
                    Ok(false) => {
                        return Err(ParseError::IncompleteRequestLine);
                    }
                    Err(e) => Err(e),
                },
                ParsingState::Complete => break,
                ParsingState::Error => break,
            };

            match res {
                Ok(_) => {
                    if conn.request.state == ParsingState::Complete {
                        break;
                    }
                }
                Err(ParseError::IncompleteRequestLine) => {
                    return Err(ParseError::IncompleteRequestLine);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    pub fn setup_action(conn: &mut HttpConnection) -> Option<HttpResponse> {
        let s_cfg = conn.resolve_config();
        conn.s_cfg = Some(Arc::clone(&s_cfg));

        let content_length = conn
            .request
            .headers
            .get("content-length")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        let is_chunked = conn
            .request
            .headers
            .get("transfer-encoding")
            .map(|v| v.contains("chunked"))
            .unwrap_or(false);

        let content_type = conn
            .request
            .headers
            .get("content-type")
            .map(|s| s.as_str())
            .unwrap_or("");

        conn.boundary = content_type
            .split("boundary=")
            .nth(1)
            .map(|b| b.trim())
            .unwrap_or("")
            .to_string();

        // 1. Initial Size Check
        if !is_chunked && content_length > s_cfg.client_max_body_size {
            return Some(Server::handle_error(413, Some(&s_cfg)));
        }

        conn.body_remaining = content_length;

        // 2. Resolve Route and Set Intent
        let request = &conn.request;
        let res = match s_cfg.find_route(&request.url, &request.method) {
            Ok(r_cfg) => {
                if let Some(ref redirect_url) = r_cfg.redirection {
                    Some(HttpResponse::redirect(
                        r_cfg.redirect_code.unwrap_or(HTTP_FOUND),
                        redirect_url,
                    ))
                } else if r_cfg
                    .cgi_ext
                    .as_ref()
                    .map_or(false, |ext| request.url.ends_with(ext))
                {
                    conn.action = Some(ActiveAction::Cgi(String::new()));
                    None
                } else {
                    match request.method {
                        Method::GET => Some(Server::handle_get(request, r_cfg, &s_cfg)),
                        Method::POST => {
                            // Decide if we will upload to a file
                            if !r_cfg.upload_dir.is_empty() {
                                let path = PathBuf::from(&r_cfg.root).join(&r_cfg.upload_dir);
                                conn.action = Some(ActiveAction::Upload(path));
                                None
                            } else {
                                Some(Server::handle_error(HTTP_METHOD_NOT_ALLOWED, Some(&s_cfg)))
                            }
                        }
                        Method::DELETE => Some(Server::handle_delete(request, r_cfg, &s_cfg)),
                    }
                }
            }
            Err(RoutingError::MethodNotAllowed) => {
                Some(Server::handle_error(HTTP_METHOD_NOT_ALLOWED, Some(&s_cfg)))
            }
            Err(RoutingError::NotFound) => Some(Server::handle_error(HTTP_NOT_FOUND, Some(&s_cfg))),
        };

        // 3. Update State based on body presence
        if res.is_none() {
            if is_chunked {
                conn.request.state = ParsingState::ChunkedBody(s_cfg.client_max_body_size);
            } else if content_length > 0 {
                conn.request.state = ParsingState::Body(content_length, s_cfg.client_max_body_size);
            } else {
                conn.request.state = ParsingState::Complete;
            }
        }

        res
    }

    fn parse_request_line(&mut self) -> core::result::Result<(), ParseError> {
        if let Some(abs_index) = find_crlf(&self.buffer, self.cursor) {
            // let abs_index = self.cursor + index;

            let line_bytes = &self.buffer[self.cursor..abs_index];
            let request_line =
                std::str::from_utf8(line_bytes).map_err(|_| ParseError::MalformedRequestLine)?;

            let parts: Vec<&str> = request_line.split_whitespace().collect();
            if parts.len() == 3 {
                self.method = match parts[0] {
                    "GET" => Method::GET,
                    "POST" => Method::POST,
                    "DELETE" => Method::DELETE,
                    _ => return Err(ParseError::InvalidMethod),
                };
                self.url = parts[1].to_string();
                self.version = parts[2].to_string();

                self.cursor = abs_index + CRLN_LEN;
                self.state = ParsingState::Headers;
            } else {
                return Err(ParseError::MalformedRequestLine);
            }
        } else {
            return Err(ParseError::IncompleteRequestLine);
        }
        Ok(())
    }

    fn extract_and_parse_header(
        &mut self,
    ) -> core::result::Result<Option<(String, String)>, ParseError> {
        if let Some(abs_index) = find_crlf(&self.buffer, self.cursor) {
            let line_bytes = &self.buffer[self.cursor..abs_index];
            if line_bytes.is_empty() {
                self.cursor = abs_index + CRLN_LEN;
                return Ok(None);
            }
            let line =
                std::str::from_utf8(line_bytes).map_err(|_| ParseError::MalformedRequestLine)?;
            self.cursor = abs_index + CRLN_LEN;
            if let Some(sep) = line.find(':') {
                let key = line[..sep].trim().to_string();
                let val = line[sep + 1..].trim().to_string();
                return Ok(Some((key.to_ascii_lowercase(), val)));
            }
            Err(ParseError::MalformedRequestLine)
        } else {
            Err(ParseError::IncompleteRequestLine)
        }
    }

    fn parse_headers(conn: &mut HttpConnection) -> core::result::Result<(), ParseError> {
        loop {
            let headers_option = conn.request.extract_and_parse_header()?;
            match headers_option {
                Some((k, v)) => conn.request.headers.insert(k, v),
                None => {
                    conn.request.state = ParsingState::HeadersDone;
                    return Ok(());
                }
            };
        }
    }

    pub fn parse_unchunked_body(
        conn: &mut HttpConnection,
        content_length: usize,
    ) -> core::result::Result<(), ParseError> {
        if let Some(s_cfg) = &conn.s_cfg {
            let available = conn.request.buffer.len() - conn.request.cursor;
            let to_process = std::cmp::min(available, conn.body_remaining);

            if to_process > 0 {
                let start = conn.request.cursor;
                // let chunk = &conn.request.buffer[start..start + to_process];

                HttpRequest::execute_active_action(conn, start, to_process)?;

                conn.body_remaining -= to_process;
                conn.request.buffer.drain(start..start + to_process);
            }
        }

        if conn.body_remaining == 0 {
            conn.request.state = ParsingState::Complete;
        } else {
            return Err(ParseError::IncompleteRequestLine);
        }

        Ok(())
    }

    pub fn parse_chunked_body(
        &mut self,
        max_body_size: usize,
    ) -> core::result::Result<bool, ParseError> {
        loop {
            let current_slice = &self.buffer[self.cursor..];

            match find_subsequence(current_slice, b"\r\n", 0) {
                Some(line_end) => {
                    let hex_str = String::from_utf8_lossy(&current_slice[..line_end]);
                    let chunk_size = usize::from_str_radix(hex_str.trim(), 16)
                        .map_err(|_| ParseError::ParseHexError)?;

                    let total_chunk_needed = line_end + 2 + chunk_size + 2;
                    if current_slice.len() < total_chunk_needed {
                        return Ok(false); // Wait for more data
                    }

                    if chunk_size == 0 {
                        self.cursor += total_chunk_needed;
                        return Ok(true); // Chunked body finished!
                    }

                    if self.body.len() + chunk_size > max_body_size {
                        return Err(ParseError::PayloadTooLarge);
                    }

                    let data_start = self.cursor + line_end + 2;
                    self.body
                        .extend_from_slice(&self.buffer[data_start..data_start + chunk_size]);

                    // 4. Move cursor past everything (size\r\n + data + \r\n)
                    self.cursor += total_chunk_needed;
                }
                None => return Ok(false),
            }
        }
    }

    pub fn execute_active_action(
        conn: &mut HttpConnection,
        start: usize,
        to_process: usize,
    ) -> Result<(), ParseError> {
        let chunk = &conn.request.buffer[start..start + to_process];
        if let Some(s_cfg) = &conn.s_cfg {
            match &conn.action {
                Some(ActiveAction::Upload(upload_path)) => {
                    if conn.upload_manager.is_none() {
                        conn.upload_manager = Some(Upload {
                            state: UploadState::SavedFilenames(Vec::new()), // Changed to track progress
                            path: upload_path,
                        });
                    }

                    if let Some(ref mut mgr) = conn.upload_manager {
                        if !conn.boundary.is_empty() {
                            mgr.handle_upload_3(
                                &conn.request,
                                &mgr.path.clone(),
                                conn.s_cfg.as_ref().unwrap(),
                                chunk,
                                conn.boundary
                            );
                        } else {
                            mgr.handle_upload_2(
                                &conn.request,
                                &mgr.path.clone(),
                                conn.s_cfg.as_ref().unwrap(),
                                chunk,
                            );
                        }
                    }
                }
                Some(ActiveAction::Cgi(_)) => {
                    // Future: write to child process stdin
                }
                Some(ActiveAction::Discard) => {}
                None => {
                    // If it's a small normal POST, keep in RAM
                    // conn.request.body.extend_from_slice(chunk);
                }
            }
        }

        Ok(())
    }

    pub fn extract_filename(&self) -> String {
        format!(
            "uploaded_{}",
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
                .to_string()
        )
    }

    fn sanitize_filename(name: String) -> String {
        let path = std::path::Path::new(&name);
        // 1. Take only the file name part, ignore any paths they sent
        let leaf = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("default_upload");

        // 2. Replace spaces or weird characters if you want to be extra safe
        leaf.replace(|c: char| !c.is_alphanumeric() && c != '.', "_")
    }
}

fn find_crlf(buffer: &[u8], start_offset: usize) -> Option<usize> {
    let search_area = buffer.get(start_offset..)?;

    let mut current_pos = 0;
    while let Some(r_pos) = search_area[current_pos..].iter().position(|&b| b == b'\r') {
        let abs_r_pos_in_search = current_pos + r_pos;

        if search_area.get(abs_r_pos_in_search + 1) == Some(&b'\n') {
            // Return the absolute position in the original 'buffer'
            return Some(start_offset + abs_r_pos_in_search);
        }
        current_pos = abs_r_pos_in_search + 1;
    }
    None
}

pub fn find_subsequence(buffer: &[u8], needle: &[u8], start_offset: usize) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    let search_area = buffer.get(start_offset..)?;
    let first_byte = needle[0];
    let mut current_pos = 0;

    // Use .iter().position() to find the first byte efficiently
    while let Some(rel_pos) = search_area[current_pos..]
        .iter()
        .position(|&b| b == first_byte)
    {
        let abs_pos_in_search = current_pos + rel_pos;

        // Check if the remaining bytes match
        if let Some(candidate) =
            search_area.get(abs_pos_in_search..abs_pos_in_search + needle.len())
        {
            if candidate == needle {
                return Some(start_offset + abs_pos_in_search);
            }
        } else {
            // Not enough bytes left in search_area to match needle
            return None;
        }

        // Move forward to keep searching
        current_pos = abs_pos_in_search + 1;
    }
    None
}

impl Display for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "--- HTTP Request ---\n")?;
        // 1. Request Line: GET /path HTTP/1.1
        writeln!(f, "{:?} {} {}", self.method, self.url, self.version)?;

        // 2. Headers: Key: Value
        writeln!(f, "Headers:")?;
        for (key, value) in &self.headers {
            writeln!(f, "  {}: {}", key, value)?;
        }

        // 3. Body Summary
        // We only print the body if it's UTF-8; otherwise, we show the byte count.
        if !self.body.is_empty() {
            writeln!(f, "Body ({} bytes):", self.body.len())?;
            match String::from_utf8(self.body.clone()) {
                Ok(s) => writeln!(f, "  {}", s)?,
                Err(_) => writeln!(f, "  <binary data>")?,
            }
        } else {
            writeln!(f, "Body: <empty>")?;
        }
        writeln!(f, "\n--------------------")?;
        writeln!(f, "--------------------")
    }
}

#[derive(Debug)]
pub struct PartInfo {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: String,
}

pub fn parse_part_headers(headers: &str) -> PartInfo {
    let mut info = PartInfo {
        name: String::new(),
        filename: None,
        content_type: String::new(),
    };

    for line in headers.lines() {
        if line.starts_with("Content-Disposition:") {
            // Extract 'name'
            if let Some(n) = line.split(';').find(|s| s.trim().starts_with("name=")) {
                info.name = n
                    .split('=')
                    .nth(1)
                    .unwrap_or("")
                    .trim_matches('"')
                    .to_string();
            }
            // Extract 'filename'
            if let Some(f) = line.split(';').find(|s| s.trim().starts_with("filename=")) {
                info.filename = Some(
                    f.split('=')
                        .nth(1)
                        .unwrap_or("")
                        .trim_matches('"')
                        .to_string(),
                );
            }
        } else if line.starts_with("Content-Type:") {
            info.content_type = line
                .split(':')
                .nth(1)
                .unwrap_or("text/plain")
                .trim()
                .to_string();
        }
    }
    info
}
