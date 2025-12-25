use std::{
    cmp,
    collections::HashMap,
    fmt::{self, Display},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Method {
    GET,
    POST,
    DELETE,
}

#[derive(Debug, PartialEq)]
pub enum ParsingState {
    RequestLine,
    Headers,
    Body(usize), // Content-Length
    Complete,
    Error,
}

/*
BodyContentLength {
        remaining: usize,
        max_size: usize,
    },
    // Required for chunked requests
    BodyChunked {
        current_chunk_size: usize,
        max_size: usize,
        total_read: usize,
    },
 */

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
                ParsingState::Body(content_length) => self.parse_body(content_length),
                _ => return Ok(()),
            };

            if let Err(ParseError::IncompleteRequestLine) = res {
                return Err(ParseError::IncompleteRequestLine);
            }

            res?;
        }
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
                dbg!("ss");
                self.cursor = abs_index + CRLN_LEN;
                return Ok(None);
            }
            let line =
                std::str::from_utf8(line_bytes).map_err(|_| ParseError::MalformedRequestLine)?;
            self.cursor = abs_index + CRLN_LEN;
            dbg!(&line);
            if let Some(sep) = line.find(':') {
                let key = line[..sep].trim().to_string();
                let val = line[sep + 1..].trim().to_string();
                dbg!("ehh");
                return Ok(Some((key, val)));
            }
            dbg!("kkkkk");

            Err(ParseError::MalformedRequestLine)
        } else {
            dbg!("99999");
            Err(ParseError::IncompleteRequestLine)
        }
    }

    fn parse_headers(&mut self) -> core::result::Result<(), ParseError> {
        loop {
            let headers_option = self.extract_and_parse_header()?;
            match headers_option {
                Some((k, v)) => self.headers.insert(k, v),
                None => {
                    let content_length = self
                        .headers
                        .get("Content-Length")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);

                    if content_length > 0 {
                        self.state = ParsingState::Body(content_length);
                    } else {
                        self.state = ParsingState::Complete;
                    }
                    return Ok(());
                }
            };
        }
    }

    pub fn parse_body(&mut self, content_length: usize) -> core::result::Result<(), ParseError> {
        let available = self.buffer.len() - self.cursor;

        if available < content_length {
            return Err(ParseError::IncompleteRequestLine); // Need more data
        }
        self.body = self.buffer[self.cursor..self.cursor + content_length].to_vec();
        self.cursor += content_length;
        self.state = ParsingState::Complete;

        Ok(())
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
impl Display for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "--- HTTP Request ---")?;
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
        writeln!(f, "--------------------")
    }
}
