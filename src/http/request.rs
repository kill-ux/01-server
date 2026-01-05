use std::{
    collections::HashMap,
    fmt::{self, Display},
    fs::File,
    str::FromStr,
    time::SystemTime,
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
    ErrorCreateTempFile
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
            ParseError::ErrorCreateTempFile => write!(f, "Error Create Temp File")
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

    pub fn parse_request(&mut self) -> core::result::Result<(), ParseError> {
        loop {
            let res = match self.state {
                ParsingState::RequestLine => self.parse_request_line(),
                ParsingState::Headers => self.parse_headers(),
                ParsingState::Body(content_length, max_body_size) => {
                    self.parse_unchunked_body(content_length, max_body_size)
                }
                ParsingState::ChunkedBody(max_body_size) => {
                    match self.parse_chunked_body(max_body_size) {
                        Ok(true) => {
                            self.state = ParsingState::Complete;
                            Ok(())
                        }
                        Ok(false) => {
                            return Err(ParseError::IncompleteRequestLine);
                        }
                        Err(e) => Err(e),
                    }
                }
                ParsingState::Complete => return Ok(()),
                _ => return Ok(()),
            };

            if let Err(ParseError::IncompleteRequestLine) = res {
                return Err(ParseError::IncompleteRequestLine);
            }

            res?;

            if self.state == ParsingState::Complete {
                break;
            }
        }
        Ok(())
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

    fn parse_headers(&mut self) -> core::result::Result<(), ParseError> {
        loop {
            let headers_option = self.extract_and_parse_header()?;
            match headers_option {
                Some((k, v)) => self.headers.insert(k, v),
                None => {
                    self.state = ParsingState::HeadersDone;
                    return Ok(());
                }
            };
        }
    }

    pub fn parse_unchunked_body(
        &mut self,
        content_length: usize,
        max_body_size: usize,
    ) -> core::result::Result<(), ParseError> {
        if content_length > max_body_size {
            return Err(ParseError::PayloadTooLarge);
        }

        if content_length > _1MB {
            if self.body_file.is_none() {
                let temp_path = format!("tmp_body_{}.tmp", self.extract_filename());
                self.body_file = Some(File::create(temp_path));
            }
        } else {
            let available = self.buffer.len() - self.cursor;

            if available < content_length {
                return Err(ParseError::IncompleteRequestLine);
            }
            self.body = self.buffer[self.cursor..self.cursor + content_length].to_vec();
            self.cursor += content_length;
            self.state = ParsingState::Complete;
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
