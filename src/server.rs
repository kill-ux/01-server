use crate::config::{AppConfig, RouteConfig, ServerConfig};
use crate::error::Result;
use crate::http::*;
use crate::router::RoutingError;
use mio::{
    Events, Interest, Poll, Token,
    event::Event,
    net::{TcpListener, TcpStream},
};
use proxy_log::info;
use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const READ_BUF_SIZE: usize = 4096;
// 4xx Client Errors
const HTTP_BAD_REQUEST: u16 = 400;
const HTTP_FORBIDDEN: u16 = 403;
const HTTP_NOT_FOUND: u16 = 404;
const HTTP_METHOD_NOT_ALLOWED: u16 = 405;
const HTTP_PAYLOAD_TOO_LARGE: u16 = 413;
const HTTP_URI_TOO_LONG: u16 = 414;

// 5xx Server Errors
const HTTP_INTERNAL_SERVER_ERROR: u16 = 500;
const HTTP_NOT_IMPLEMENTED: u16 = 501;

const HTTP_FOUND: u16 = 302;

pub struct HttpConnection {
    pub stream: TcpStream,
    pub write_buffer: Vec<u8>,
    pub request: HttpRequest,
    pub config_list: Vec<Arc<ServerConfig>>,
    pub s_cfg: Option<Arc<ServerConfig>>,
}

impl HttpConnection {
    pub fn new(stream: TcpStream, config_list: Vec<Arc<ServerConfig>>) -> Self {
        Self {
            stream,
            write_buffer: Vec::new(),
            request: HttpRequest::new(),
            config_list,
            s_cfg: None,
        }
    }

    pub fn resolve_config(&self) -> Arc<ServerConfig> {
        if let Some(host_header) = self.request.headers.get("Host") {
            let hostname = host_header.split(':').next().unwrap_or("");

            for config in &self.config_list {
                if config.server_name == hostname {
                    return Arc::clone(config);
                }
            }
        }

        // If no match found, find the default_server, or fallback to the first one
        for config in &self.config_list {
            if config.default_server {
                return Arc::clone(config);
            }
        }

        // Fallback to the first one
        Arc::clone(&self.config_list[0])
    }
}
impl HttpConnection {
    // Returns true if the connection should be closed
    fn read_data(&mut self) -> core::result::Result<bool, ParseError> {
        let mut buf = [0u8; READ_BUF_SIZE]; // READ_BUF_SIZE
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => return Ok(true), // EOF
                Ok(n) => {
                    // let absolute_limit: usize = 16_384;
                    // if self.request.buffer.len() + n > absolute_limit {
                    //     return Err(ParseError::PayloadTooLarge);
                    // }
                    self.request.buffer.extend_from_slice(&buf[..n]);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => return Ok(true),
            }
        }
        Ok(false)
    }

    fn write_data(&mut self) -> bool {
        match self.stream.write(&self.write_buffer) {
            Ok(n) => {
                self.write_buffer.drain(..n);
                false
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => false,
            Err(_) => true,
        }
    }
}

pub struct Server {
    pub listeners: HashMap<Token, (TcpListener, Vec<Arc<ServerConfig>>)>,
    pub connections: HashMap<Token, HttpConnection>,
    next_token: usize,
}

impl Server {
    pub fn new(config: AppConfig, poll: &Poll) -> Result<Self> {
        let mut listeners = HashMap::new();
        let mut next_token = 0;

        info!("Initializing server listeners...");

        let mut groups: HashMap<(String, u16), Vec<Arc<ServerConfig>>> = HashMap::new();

        for s_cfg in config.servers {
            let shared_s_cfg = Arc::new(s_cfg);
            for &port in &shared_s_cfg.ports {
                let key = (shared_s_cfg.host_header(), port);
                groups
                    .entry(key)
                    .or_default()
                    .push(Arc::clone(&shared_s_cfg));
            }
        }

        for ((host, port), config_list) in groups {
            let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
            let token = Token(next_token);

            let mut listener = TcpListener::bind(addr)?;
            poll.registry()
                .register(&mut listener, token, Interest::READABLE)?;
            listeners.insert(token, (listener, config_list));

            next_token += 1;
        }

        Ok(Self {
            listeners,
            connections: HashMap::new(),
            next_token: next_token + 1,
        })
    }

    pub fn run(&mut self, mut poll: Poll) -> Result<()> {
        let mut events = Events::with_capacity(1024);

        println!(
            "Server running. Monitoring {} listeners...",
            self.listeners.len()
        );

        loop {
            // Wait for events
            poll.poll(&mut events, None)?;

            for event in events.iter() {
                let token = event.token();

                // 1. Handle New Connections
                if self.listeners.contains_key(&token) {
                    if let Err(e) = self.handle_accept(&mut poll, token) {
                        eprintln!("Accept Error: {}", e);
                    }
                }
                // 2. Handle Existing Connection Data
                else if let Err(e) = self.handle_connection(&poll, event, token) {
                    eprintln!("Connection Error: {}", e);
                    // The removal is already handled inside handle_connection or here
                    self.connections.remove(&token);
                }
            }
        }
    }

    pub fn handle_accept(&mut self, poll: &mut Poll, token: Token) -> Result<()> {
        let (listener, config_list) = self.listeners.get_mut(&token).unwrap();

        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let client_token = Token(self.next_token);
                    self.next_token += 1;
                    poll.registry()
                        .register(&mut stream, client_token, Interest::READABLE)?;
                    let conn = HttpConnection::new(stream, config_list.clone());
                    self.connections.insert(client_token, conn);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn handle_connection(&mut self, poll: &Poll, event: &Event, token: Token) -> Result<()> {
        let mut closed = false;

        if let Some(conn) = self.connections.get_mut(&token) {
            if event.is_readable() {
                match conn.read_data() {
                    Ok(is_eof) => closed = is_eof,
                    Err(ParseError::PayloadTooLarge) => {
                        let error_res = "HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        conn.write_buffer.extend_from_slice(error_res.as_bytes());
                        // Change interest to writable to send the error before closing
                        poll.registry()
                            .reregister(&mut conn.stream, token, Interest::WRITABLE)?;
                        return Ok(());
                    }
                    Err(_) => closed = true,
                };

                if !closed && !conn.request.buffer.is_empty() {
                    // conn.request.state != ParsingState::Complete
                    // Call parsing/routing logic

                    Self::proces_request(poll, token, conn)?;
                }
            }

            if !closed && event.is_writable() && !conn.write_buffer.is_empty() {
                closed = conn.write_data();
                if !closed && conn.write_buffer.is_empty() {
                    poll.registry()
                        .reregister(&mut conn.stream, token, Interest::READABLE)?;
                }
            }

            if closed || event.is_read_closed() || event.is_write_closed() {
                // Borrow ends here, so we can remove safely below
            } else {
                return Ok(()); // Keep connection alive
            }
        }

        self.connections.remove(&token);
        Ok(())
    }

    fn proces_request(poll: &Poll, token: Token, conn: &mut HttpConnection) -> Result<()> {
        println!("### start processing a request ###");
        loop {
            match conn.request.parse_request() {
                Ok(()) => {
                    if conn.request.state == ParsingState::HeadersDone {
                        let s_cfg = conn.resolve_config();
                        conn.s_cfg = Some(Arc::clone(&s_cfg));

                        let content_length = conn
                            .request
                            .headers
                            .get("Content-Length")
                            .and_then(|s| s.parse::<usize>().ok())
                            .unwrap_or(0);

                        if content_length > 0 {
                            if content_length > s_cfg.client_max_body_size {
                                // return Err(ParseError::PayloadTooLarge.into());
                            }
                            conn.request.state =
                                ParsingState::Body(content_length, s_cfg.client_max_body_size);
                            continue;
                        } else {
                            conn.request.state = ParsingState::Complete;
                        }
                    }

                    if conn.request.state == ParsingState::Complete {
                        let request = &conn.request;
                        let s_cfg = conn.s_cfg.as_ref().unwrap();

                        // Perform Routing
                        let response = match s_cfg.find_route(&request.url, &request.method) {
                            Ok(r_cfg) => {
                                if let Some(ref redirect_url) = r_cfg.redirection {
                                    let code = r_cfg.redirect_code.unwrap_or(HTTP_FOUND);
                                    HttpResponse::redirect(code, redirect_url)
                                } else if r_cfg
                                    .cgi_ext
                                    .as_ref()
                                    .map_or(false, |ext| request.url.ends_with(ext))
                                {
                                    Server::handle_cgi(request, r_cfg)
                                } else {
                                    Server::handle_static_file(request, r_cfg, s_cfg)
                                }
                            }
                            Err(RoutingError::MethodNotAllowed) => {
                                Self::handle_error(HTTP_METHOD_NOT_ALLOWED, conn.s_cfg.as_ref())
                            }
                            Err(RoutingError::NotFound) => {
                                Self::handle_error(HTTP_NOT_FOUND, Some(s_cfg))
                            }
                        };

                        conn.write_buffer.extend_from_slice(&response.to_bytes());
                        conn.request.finish_request(); // Clean up for next request

                        if conn.request.buffer.is_empty() {
                            break;
                        }
                    } else {
                        // Still in RequestLine, Headers, or Body state but buffer is empty
                        break;
                    }
                }
                Err(ParseError::IncompleteRequestLine) => break,
                Err(e) => {
                    let code = match e {
                        ParseError::PayloadTooLarge => HTTP_PAYLOAD_TOO_LARGE,
                        ParseError::InvalidMethod => HTTP_METHOD_NOT_ALLOWED,
                        ParseError::HeaderTooLong => HTTP_URI_TOO_LONG,
                        _ => HTTP_BAD_REQUEST,
                    };
                    let response = Self::handle_error(code, conn.s_cfg.as_ref());
                    conn.write_buffer.extend_from_slice(&response.to_bytes());
                    conn.request.finish_request();
                    break;
                }
            }
        }

        if conn.request.state != ParsingState::Complete {
            println!(
                "Request is still partial (State: {:?}). Waiting for more data...",
                conn.request.state
            );
        }

        if !conn.write_buffer.is_empty() {
            poll.registry().reregister(
                &mut conn.stream,
                token,
                Interest::READABLE | Interest::WRITABLE,
            )?;
        }
        Ok(())
    }

    pub fn handle_cgi(_request: &HttpRequest, _r_cfg: &RouteConfig) -> HttpResponse {
        HttpResponse::new(200, "OK").set_body(b"Hello World".to_vec(), "text/plain")
    }

    pub fn handle_static_file(
        request: &HttpRequest,
        r_cfg: &RouteConfig,
        s_cfg: &Arc<ServerConfig>,
    ) -> HttpResponse {
        println!("{request}");

        let root = &r_cfg.root;
        let relative_path = request
            .url
            .strip_prefix(&r_cfg.path)
            .unwrap_or(&request.url);
        let mut path = PathBuf::from(root);
        path.push(relative_path.trim_start_matches('/'));

        dbg!(&path);

        if path.is_dir() {
            if r_cfg.default_file != "" {
                path.push(&r_cfg.default_file);
            } else if r_cfg.autoindex {
                return Self::generate_autoindex(&path, &request.url);
            } else {
                return HttpResponse::new(403, "Forbidden").set_body(
                    b"403 Forbidden: Directory listing denied".to_vec(),
                    "text/plain",
                );
            }
        }

        match fs::read(&path) {
            Ok(content) => {
                let mime_type = Self::get_mime_type(path.extension().and_then(|s| s.to_str()));
                HttpResponse::new(200, "OK").set_body(content, mime_type)
            }
            Err(e) => match e.kind() {
                std::io::ErrorKind::NotFound => Self::handle_error(HTTP_NOT_FOUND, Some(s_cfg)),
                std::io::ErrorKind::PermissionDenied => {
                    Self::handle_error(HTTP_FORBIDDEN, Some(s_cfg))
                }
                _ => Self::handle_error(HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg)),
            },
        }
    }

    fn get_mime_type(extension: Option<&str>) -> &'static str {
        match extension {
            Some("html") | Some("htm") => "text/html",
            Some("css") => "text/css",
            Some("js") => "application/javascript",
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("json") => "application/json",
            Some("txt") => "text/plain",
            _ => "application/octet-stream",
        }
    }

    fn generate_autoindex(path: &Path, original_url: &str) -> HttpResponse {
        let mut html = format!("<html><body><h1>Index of {}</h1><ul>", original_url);
        if let Ok(entries) = path.read_dir() {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    html.push_str(&format!(
                        "<li><a href=\"{}/{}\">{}</a></li>",
                        original_url.trim_end_matches('/'),
                        name,
                        name
                    ));
                }
            }
        }

        html.push_str("</ul></body></html>");
        HttpResponse::new(200, "OK").set_body(html.into_bytes(), "text/html")
    }

    pub fn handle_error(code: u16, s_cfg: Option<&Arc<ServerConfig>>) -> HttpResponse {
        let status_text = match code {
            HTTP_BAD_REQUEST => "Bad Request",
            HTTP_FORBIDDEN => "Forbidden",
            HTTP_NOT_FOUND => "Not Found",
            HTTP_METHOD_NOT_ALLOWED => "Method Not Allowed",
            HTTP_PAYLOAD_TOO_LARGE => "Payload Too Large",
            HTTP_URI_TOO_LONG => "URI Too Long",
            HTTP_NOT_IMPLEMENTED => "Not Implemented",
            _ => "Internal Server Error",
        };

        if let Some(cfg) = s_cfg {
            if let Some(path_str) = cfg.error_pages.get(&code) {
                if let Ok(content) = fs::read(path_str) {
                    return HttpResponse::new(code, status_text).set_body(content, "text/html");
                }
            }
        }

        let body = format!("{} {}", code, status_text).into_bytes();
        HttpResponse::new(code, status_text).set_body(body, "text/plain")
    }
}
