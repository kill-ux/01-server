use std::{collections::HashMap, error::Error, fmt};

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

impl Error for ParseError {}

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
            buffer: Vec::new(),
            state: ParsingState::RequestLine,
        }
    }
}

pub fn parse_request(request: &mut HttpRequest) -> Result<(), ParseError> {
    loop {
        match request.state {
            ParsingState::RequestLine => parse_request_line(request)?,
            ParsingState::Headers => parse_headers(request)?,
            _ => return Ok(()),
        }
    }
}

fn parse_request_line(request: &mut HttpRequest) -> Result<(), ParseError> {
    if let Some(index) = find_crlf(&request.buffer) {
        let request_line_bytes = request.buffer.drain(..index + 2).collect::<Vec<u8>>();
        let request_line = String::from_utf8(request_line_bytes)?;

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() == 3 {
            request.method = match parts[0] {
                "GET" => Method::GET,
                "POST" => Method::POST,
                "DELETE" => Method::DELETE,
                _ => return Err(ParseError::InvalidMethod),
            };
            request.url = parts[1].to_string();
            request.version = parts[2].to_string();
            request.state = ParsingState::Headers;
        } else {
            return Err(ParseError::MalformedRequestLine);
        }
    } else {
        return Err(ParseError::IncompleteRequestLine);
    }
    Ok(())
}

fn extract_and_parse_header(
    request: &mut HttpRequest,
) -> Result<Option<(String, String)>, ParseError> {
    if let Some(index) = find_crlf(&request.buffer) {
        if index == 0 {
            return Ok(None);
        }
        let row = request.buffer.drain(..index + 2).collect::<Vec<u8>>();
        let key_value_string = String::from_utf8(row)?;
        return match key_value_string.trim_end_matches("\n\r").find(":") {
            Some(index) => {
                return Ok(Some((
                    key_value_string[..index].trim().to_string(),
                    key_value_string[index + 1..].trim().to_string(),
                )));
            }
            None => Err(ParseError::MalformedRequestLine),
        };
    } else {
        Err(ParseError::IncompleteRequestLine)
    }
    
}

fn parse_headers(request: &mut HttpRequest) -> Result<(), ParseError> {
    loop {
        let headers_option = extract_and_parse_header(request)?;
        match headers_option {
            Some((k, v)) => request.headers.insert(k, v),
            None => {
                 match request.headers.get("Content-Length") {
                Some(content_length_str) => {
                    if let Ok(size) = content_length_str.parse::<usize>() {
                        request.state = ParsingState::Body(size);
                        return Ok(());
                    } else {
                        return Err(ParseError::InvalidHeaderValue);
                    }
                }
                None => return Err(ParseError::MalformedRequestLine)
            }
            },
        };
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

fn main() {
    let http_get = concat!(
        "GET /hello.htm HTTP/1.1\r\n",
        "Host: www.tutorialspoint.com\r\n",
        "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36\r\n",
        "Accept-Language: en-us\r\n",
        "Connection: Keep-Alive\r\n",
        "Content-Length: 5\r\n",
        "\r\n",
        "Hello"
    );

    let mut request = HttpRequest::new();
    request.buffer.extend_from_slice(http_get.as_bytes());

    match parse_request(&mut request) {
        Ok(()) => println!("Parsed: {:?}", request),
        Err(e) => {
            eprintln!("Parse error: {}", e)
        }
    }
}

// let mut poll = Poll::new()?;
// let mut events = Events::with_capacity(128);

// let addr = "127.0.0.1:13265".parse()?;
// let mut server = TcpListener::bind(addr)?;

// poll.registry().register(&mut server, SERVER, Interest::READABLE)?;

// let mut client = TcpStream::connect(addr)?;
// poll.registry().register(&mut client, CLIENT, Interest::READABLE | Interest::WRITABLE)?;

// loop {
//     poll.poll(&mut events, None)?;

//     for event in events.iter() {
//         match event.token() {
//             SERVER => {
//                 let con = server.accept()?;
//                 drop(con);
//             }

//             CLIENT => {
//                 if event.is_readable() {

//                 }

//                 if event.is_writable() {

//                 }

//                 return Ok(());
//             }

//             _ => unreachable!()
//         }
//     }
// }
