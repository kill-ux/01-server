use std::{collections::HashMap, fmt};

#[derive(Debug, Clone, PartialEq)]
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
    pub query_params: HashMap<String, String>,
    pub buffer: Vec<u8>,
    pub state: ParsingState,
}

impl HttpRequest {
    pub fn new() -> Self {
        HttpRequest {
            method: Method::GET,
            url: String::new(),
            version: String::new(),
            headers: HashMap::new(),
            body: Vec::new(),
            query_params: HashMap::new(),
            buffer: Vec::with_capacity(4096),
            state: ParsingState::RequestLine,
        }
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
        println!(
            "####{}####\n",
            String::from_utf8(self.buffer.clone()).unwrap()
        );
        if let Some(index) = find_crlf(&self.buffer) {
            let request_line_bytes = self.buffer.drain(..index).collect::<Vec<u8>>();
            self.buffer.drain(..2);
            let request_line = String::from_utf8(request_line_bytes)?;
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
        if let Some(index) = find_crlf(&self.buffer) {
            if index == 0 {
                self.buffer.drain(..2);
                return Ok(None);
            }
            let row: Vec<u8> = self.buffer.drain(..index + 2).collect::<Vec<u8>>();
            let key_value_string = String::from_utf8(row)?;

            return match key_value_string.trim_end_matches("\r\n").find(":") {
                Some(index) => {
                    return Ok(Some((
                        key_value_string[..index].trim().to_string(),
                        key_value_string[index + 1..].trim().to_string(),
                    )));
                }
                None => {
                    println!("here");
                    Err(ParseError::MalformedRequestLine)
                }
            };
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
                    if !(self.method == Method::GET) {
                        match self.headers.get("Content-Length") {
                            Some(content_length_str) => {
                                if let Ok(size) = content_length_str.parse::<usize>() {
                                    self.state = ParsingState::Body(size);
                                    return Ok(());
                                } else {
                                    return Err(ParseError::InvalidHeaderValue);
                                }
                            }
                            None => {
                                println!("nnnnnnn");
                                return Err(ParseError::MalformedRequestLine);
                            }
                        }
                    }
                    self.state = ParsingState::Complete;
                    return Ok(());
                }
            };
        }
    }

    pub fn parse_body(&mut self, content_length: usize) -> core::result::Result<(), ParseError> {
        if self.buffer.len() < content_length {
            // Not enough data yet
            return Err(ParseError::IncompleteRequestLine);
        }
        self.body = self.buffer.drain(..content_length).collect();
        self.state = ParsingState::Complete;
        Ok(())
    }
}

fn find_crlf(rows: &[u8]) -> Option<usize> {
    for index in 0..rows.len().saturating_sub(1) {
        if rows[index] == b'\r' && rows[index + 1] == b'\n' {
            return Some(index);
        }
    }
    None
}
