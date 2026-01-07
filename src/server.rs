use crate::config::{AppConfig, RouteConfig, ServerConfig};
use crate::error::Result;
use crate::http::*;
use mio::{
    Events, Interest, Poll, Token,
    event::Event,
    net::{TcpListener, TcpStream},
};
use proxy_log::info;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub const READ_BUF_SIZE: usize = 4096;
// 4xx Client Errors
pub const HTTP_BAD_REQUEST: u16 = 400;
pub const HTTP_FORBIDDEN: u16 = 403;
pub const HTTP_NOT_FOUND: u16 = 404;
pub const HTTP_METHOD_NOT_ALLOWED: u16 = 405;
pub const HTTP_PAYLOAD_TOO_LARGE: u16 = 413;
pub const HTTP_URI_TOO_LONG: u16 = 414;

// 5xx Server Errors
pub const HTTP_INTERNAL_SERVER_ERROR: u16 = 500;
pub const HTTP_NOT_IMPLEMENTED: u16 = 501;

pub const HTTP_FOUND: u16 = 302;
pub const HTTP_CREATED: u16 = 201;

#[derive(Debug)]
pub struct HttpConnection {
    pub stream: TcpStream,
    pub write_buffer: Vec<u8>,
    pub request: HttpRequest,
    pub config_list: Vec<Arc<ServerConfig>>,
    pub s_cfg: Option<Arc<ServerConfig>>,
    pub action: Option<ActiveAction>,
    pub upload_manager: Option<Upload>,
    pub total_body_read: usize,
    pub body_remaining: usize,
    pub boundary: String,
    pub closed: bool,
    pub linger_until: Option<Instant>
}

#[derive(Debug)]
pub enum ActiveAction {
    Upload(PathBuf),
    Cgi(String), // we use ChildStdin
    Discard,
}

#[derive(Debug)]
pub enum UploadState {
    InProgress,
    Done,
    Error(u16),
}

impl Upload {
    pub fn new(path: PathBuf, boundary: &str) -> Self {
        Self {
            state: UploadState::InProgress,
            multi_part_state: MultiPartState::Start,
            path,
            boundary: boundary.to_string(),
            buffer: Vec::new(),
            current_pos: 0,
            saved_filenames: Vec::new(),
            files_saved: 0,
            part_info: PartInfo::default(),
            current_file_path: None,
        }
    }
}

#[derive(Debug)]
pub struct Upload {
    pub state: UploadState,
    pub multi_part_state: MultiPartState,
    pub path: PathBuf,
    pub boundary: String,
    pub buffer: Vec<u8>,
    pub current_pos: usize,
    pub saved_filenames: Vec<String>,
    pub files_saved: usize,
    pub part_info: PartInfo,
    pub current_file_path: Option<PathBuf>,
}

#[derive(Debug)]
pub enum MultiPartState {
    Start,
    HeaderSep,
    NextBoundary(usize),
}

impl HttpConnection {
    pub fn new(stream: TcpStream, config_list: Vec<Arc<ServerConfig>>) -> Self {
        Self {
            stream,
            write_buffer: Vec::new(),
            request: HttpRequest::new(),
            upload_manager: None,
            config_list,
            s_cfg: None,
            action: None,
            total_body_read: 0,
            body_remaining: 0,
            boundary: String::new(),
            closed: false,
            linger_until: None
        }
    }

    pub fn resolve_config(&self) -> Arc<ServerConfig> {
        if let Some(host_header) = self.request.headers.get("host") {
            let hostname = host_header.split(':').next().unwrap_or("");

            for config in &self.config_list {
                if config.server_name == hostname {
                    return Arc::clone(config);
                }
            }
        }

        //  default_server
        for config in &self.config_list {
            if config.default_server {
                return Arc::clone(config);
            }
        }

        // Fallback to the first one
        Arc::clone(&self.config_list[0])
    }
}

const MAX_READ_DATA: usize = u16::MAX as usize; // 64KB

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
                    if self.request.buffer.len() >= MAX_READ_DATA {
                        break;
                    }
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
            // let timeout = self
            //     .connections
            //     .values()
            //     .filter_map(|c| c.linger_until)
            //     .map(|deadline| deadline.saturating_duration_since(std::time::Instant::now()))
            //     .min()
            //     .or(Some(std::time::Duration::from_millis(100)));

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

            // self.connections.retain(|_, conn| {
            //     if let Some(deadline) = conn.linger_until {
            //         return std::time::Instant::now() < deadline;
            //     }
            //     true
            // });

            // println!("hhhhhhhhhhhhhhh");
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
        if let Some(conn) = self.connections.get_mut(&token) {
            if !conn.closed && event.is_readable() {
                match conn.read_data() {
                    Ok(is_eof) => conn.closed = is_eof,
                    Err(ParseError::PayloadTooLarge) => {
                        let error_res = "HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        conn.write_buffer.extend_from_slice(error_res.as_bytes());
                        conn.closed = true;
                        poll.registry()
                            .reregister(&mut conn.stream, token, Interest::WRITABLE)?;
                        return Ok(());
                    }
                    Err(_) => conn.closed = true,
                };

                poll.registry()
                    .reregister(&mut conn.stream, token, Interest::READABLE)?;

                if !conn.closed && !conn.request.buffer.is_empty() && conn.write_buffer.is_empty() {
                    // conn.request.state != ParsingState::Complete
                    // Call parsing/routing logic

                    conn.closed = Self::proces_request(poll, token, conn)?;
                }
            }

            if event.is_writable() && !conn.write_buffer.is_empty() {
                conn.closed = conn.write_data() || conn.closed;
                if !conn.closed && conn.write_buffer.is_empty() {
                    poll.registry()
                        .reregister(&mut conn.stream, token, Interest::READABLE)?;

                    if !conn.request.buffer.is_empty()
                        && conn.request.state == ParsingState::RequestLine
                    {
                        println!(
                            "Write finished. Found leftover data in buffer, processing next request..."
                        );
                        conn.closed = Self::proces_request(poll, token, conn)?;
                    }
                }
            }
            if conn.closed && conn.write_buffer.is_empty() {
                // Borrow ends here, so we can remove safely below

                // conn.linger_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(10000));
            } else {
                return Ok(()); // Keep connection alive
            }
        }
        println!("remove connection");
        self.connections.remove(&token);
        Ok(())
    }

    fn proces_request(poll: &Poll, token: Token, conn: &mut HttpConnection) -> Result<bool> {
        let mut closed = false;
        dbg!("### start processing a request ###");
        loop {
            match HttpRequest::parse_request(conn) {
                Ok(()) => {
                    if conn.request.state == ParsingState::Complete {
                        println!("complete");
                        let s_cfg = conn.s_cfg.as_ref().unwrap();

                        if let Some(upload_manager) = &mut conn.upload_manager {
                            if upload_manager.boundary.is_empty() {
                                if let Some(target_path) = &upload_manager.current_file_path {
                                    upload_manager.saved_filenames.push(
                                        target_path
                                            .file_name()
                                            .unwrap()
                                            .to_string_lossy()
                                            .into_owned(),
                                    );
                                    upload_manager.files_saved += 1;
                                }
                            }
                            let response = if upload_manager.saved_filenames.len() > 0 {
                                let mut res = HttpResponse::new(HTTP_CREATED, "Created");
                                if upload_manager.saved_filenames.len() == 1 {
                                    res.headers.insert(
                                        "location".to_string(),
                                        format!("/upload/{}", upload_manager.saved_filenames[0]),
                                    );
                                    res.set_body(
                                        format!(
                                            "File saved as {}",
                                            upload_manager.saved_filenames[0]
                                        )
                                        .into_bytes(),
                                        "text/plain",
                                    )
                                } else {
                                    let body_msg = format!(
                                        "Saved files: {}",
                                        upload_manager.saved_filenames.join(", ")
                                    );
                                    res.set_body(body_msg.into_bytes(), "text/plain")
                                }
                            } else {
                                Self::handle_error(HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg))
                            };

                            conn.write_buffer.extend_from_slice(&response.to_bytes());
                        }
                        dbg!("hhhhhhhhhhhhhhhh");
                        conn.request.finish_request();
                        // conn.request.buffer.clear();

                        // if conn.request.buffer.is_empty() {
                        break;
                        // }
                    } else {
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
                    closed = true;
                    conn.write_buffer.extend_from_slice(&response.to_bytes());
                    conn.request.finish_request();
                    break;
                }
            }
        }

        if conn.request.state != ParsingState::Complete {
            // println!(
            //     "Request is still partial (State: {:?}). Waiting for more data...",
            //     conn.request.state
            // );
        }

        if !conn.write_buffer.is_empty() {
            poll.registry().reregister(
                &mut conn.stream,
                token,
                Interest::READABLE | Interest::WRITABLE,
            )?;
        }
        Ok(closed)
    }

    pub fn handle_cgi(_request: &HttpRequest, _r_cfg: &RouteConfig) -> HttpResponse {
        HttpResponse::new(200, "OK").set_body(b"Hello World".to_vec(), "text/plain")
    }

    pub fn handle_get(
        request: &HttpRequest,
        r_cfg: &RouteConfig,
        s_cfg: &Arc<ServerConfig>,
    ) -> HttpResponse {
        let root = &r_cfg.root;
        let relative_path = request
            .url
            .strip_prefix(&r_cfg.path)
            .unwrap_or(&request.url);
        let mut path = PathBuf::from(root);
        path.push(relative_path.trim_start_matches('/'));

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

    pub fn handle_delete(
        request: &HttpRequest,
        r_cfg: &RouteConfig,
        s_cfg: &Arc<ServerConfig>,
    ) -> HttpResponse {
        let upload_base = PathBuf::from(&r_cfg.root).join(&r_cfg.upload_dir);

        // e.g., /upload/test.txt -> test.txt
        let relative_path = request.url.strip_prefix(&r_cfg.path).unwrap_or("");
        let target_path = upload_base.join(relative_path.trim_start_matches('/'));

        // 3. Security: Canonicalize and Path Traversal Check
        // This prevents DELETE /upload/../../etc/passwd
        let absolute_upload_base = match upload_base.canonicalize() {
            Ok(path) => path,
            Err(_) => return Self::handle_error(HTTP_NOT_FOUND, Some(s_cfg)),
        };

        let absolute_target = match target_path.canonicalize() {
            Ok(path) => path,
            Err(e) => {
                return match e.kind() {
                    ErrorKind::NotFound => Self::handle_error(HTTP_NOT_FOUND, Some(s_cfg)),
                    _ => Self::handle_error(HTTP_FORBIDDEN, Some(s_cfg)),
                };
            }
        };

        if !absolute_target.starts_with(&absolute_upload_base) {
            return Self::handle_error(HTTP_FORBIDDEN, Some(s_cfg));
        }

        if absolute_target.is_dir() {
            return Self::handle_error(HTTP_FORBIDDEN, Some(s_cfg));
        }

        match fs::remove_file(&absolute_target) {
            Ok(_) => HttpResponse::new(204, "No Content"),
            Err(e) => match e.kind() {
                ErrorKind::PermissionDenied => Self::handle_error(HTTP_FORBIDDEN, Some(s_cfg)),
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

    fn get_ext_from_content_type(content_type: &str) -> &str {
        match content_type {
            "application/json" => ".json",
            "application/pdf" => ".pdf",
            "application/xml" => ".xml",
            "application/zip" => ".zip",
            "audio/mpeg" => ".mp3",
            "image/gif" => ".gif",
            "image/jpeg" => ".jpg",
            "image/png" => ".png",
            "image/svg+xml" => ".svg",
            "image/webp" => ".webp",
            "text/css" => ".css",
            "text/html" => ".html",
            "text/javascript" => ".js",
            "text/plain" => ".txt",
            "video/mp4" => ".mp4",
            _ => ".bin",
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
                let s_root = std::path::Path::new(&cfg.root);
                let err_path = s_root.join(path_str.trim_start_matches('/'));
                if let Ok(content) = fs::read(err_path) {
                    let mut res =
                        HttpResponse::new(code, status_text).set_body(content, "text/html");

                    if code >= 400 && code != 404 && code != 405 {
                        res.headers
                            .insert("connection".to_string(), "close".to_string());
                    } else {
                        res.headers
                            .insert("connection".to_string(), "keep-alive".to_string());
                    }

                    return res;
                }
            }
        }

        let mut res = HttpResponse::new(code, status_text);

        let body = format!("{} {}", code, status_text).into_bytes();
        if code >= 400 && code != 404 && code != 405 {
            res.headers
                .insert("connection".to_string(), "close".to_string());
        } else {
            res.headers
                .insert("connection".to_string(), "keep-alive".to_string());
        }
        res.set_body(body, "text/plain")
    }
}

pub fn sanitize_filename(name: &str) -> String {
    // 1. Use Path to extract only the file_name component
    // This handles cases like "path/to/my_file.txt" -> "my_file.txt"
    let path = std::path::Path::new(name);
    let raw_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("default_upload");

    // 2. Filter characters: Allow only Alphanumeric, dots, underscores, and hyphens
    let sanitized: String = raw_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' {
                c
            } else {
                '_' // Replace spaces or symbols with underscores
            }
        })
        .collect();

    // 3. Prevent hidden files or relative dots (e.g., "..", ".env") if desired
    // For many servers, we force the name to start with a standard character
    if sanitized.is_empty() || sanitized.starts_with('.') {
        format!("upload_{}", sanitized)
    } else {
        sanitized
    }
}

fn get_unique_path(directory: &PathBuf, filename: &str) -> PathBuf {
    let mut full_path = directory.join(filename);
    let mut counter = 1;

    // While the file exists, append a (1), (2), etc.
    while full_path.exists() {
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let new_name = if ext.is_empty() {
            format!("{}_{}", stem, counter)
        } else {
            format!("{}_{}.{}", stem, counter, ext)
        };

        full_path = directory.join(new_name);
        counter += 1;
    }
    full_path
}

impl Upload {
    pub fn handle_upload_2(&mut self, req: &HttpRequest, chunk: &[u8]) {
        let target_path = if let Some(ref path) = self.current_file_path {
            path.clone()
        } else {
            let upload_path = &self.path;
            let mut file_name = req.extract_filename();
            file_name.push_str(Server::get_ext_from_content_type(
                req.headers.get("content-type").map_or("", |v| v),
            ));
            let full_path = upload_path.join(&file_name);
            self.current_file_path = Some(full_path.clone());
            full_path
        };

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&target_path)
        {
            Ok(mut file) => match file.write_all(chunk) {
                Ok(_) => {}
                Err(_) => {
                    self.state = UploadState::Error(HTTP_INTERNAL_SERVER_ERROR);
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                self.state = UploadState::Error(HTTP_FORBIDDEN);
            }
            Err(_) => {
                self.state = UploadState::Error(HTTP_INTERNAL_SERVER_ERROR);
            }
        }
    }

    pub fn handle_upload_3(&mut self, req: &HttpRequest, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);

        let boundary_str = format!("--{}", self.boundary);
        let boundary_bytes = boundary_str.as_bytes();
        let header_sep = b"\r\n\r\n";

        loop {
            match self.multi_part_state {
                MultiPartState::Start => {
                    // Look in the buffer, not just the chunk
                    if let Some(start_idx) =
                        find_subsequence(&self.buffer, boundary_bytes, self.current_pos)
                    {
                        let part_start = start_idx + boundary_bytes.len() + 2;

                        // Check if we have enough data to check for the terminal "--"
                        if self.buffer.len() < part_start {
                            break;
                        }

                        if self.buffer.get(part_start - 2..part_start) == Some(b"--") {
                            self.state = UploadState::Done;
                            break;
                        }

                        self.current_pos = part_start;
                        self.multi_part_state = MultiPartState::HeaderSep;
                    } else {
                        // Clean up buffer: keep only the last boundary_bytes.len()
                        // in case the boundary is split between this chunk and next.
                        self.trim_buffer();
                        break;
                    }
                }

                MultiPartState::HeaderSep => {
                    if let Some(sep_idx) =
                        find_subsequence(&self.buffer, header_sep, self.current_pos)
                    {
                        let data_start = sep_idx + 4;
                        let headers_part =
                            String::from_utf8_lossy(&self.buffer[self.current_pos..data_start]);

                        self.part_info = parse_part_headers(&headers_part);
                        self.multi_part_state = MultiPartState::NextBoundary(data_start);
                        self.current_pos = data_start;
                    } else {
                        break;
                    }
                }

                MultiPartState::NextBoundary(data_start) => {
                    if let Some(next_boundary_idx) =
                        find_subsequence(&self.buffer, boundary_bytes, data_start)
                    {
                        let data_end = next_boundary_idx - 2; // Subtract \r\n
                        if self.part_info.filename.is_some() {
                            self.save_file_part(req, data_start, data_end);
                        }

                        // Move to next part and clear the buffer of what we used
                        self.buffer.drain(..next_boundary_idx);
                        self.current_pos = 0;
                        self.multi_part_state = MultiPartState::Start;
                    } else {
                        self.flush_partial_data(req);
                        break;
                    }
                }
            }
        }
    }

    fn flush_partial_data(&mut self, req: &HttpRequest) {
        let boundary_len = self.boundary.len() + 6; // --boundary\r\n

        if self.buffer.len() > boundary_len {
            let write_end = self.buffer.len() - boundary_len;
            let data_to_write = &self.buffer[..write_end];

            let target_path = if let Some(ref path) = self.current_file_path {
                path.clone()
            } else {
                let path = self
                    .get_current_part_path(req)
                    .unwrap_or_else(|| PathBuf::from("tmp_upload"));
                let unique =
                    get_unique_path(&self.path, path.file_name().unwrap().to_str().unwrap());
                self.current_file_path = Some(unique.clone());
                unique
            };

            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&target_path)
            {
                let _ = file.write_all(data_to_write);
            }

            self.buffer.drain(..write_end);
            self.current_pos = 0;
        }
    }

    fn get_current_part_path(&self, req: &HttpRequest) -> Option<PathBuf> {
        // Use the part_info to generate the path, similar to your save_file_part logic
        if self.part_info.filename.is_none() {
            return None;
        }

        let raw_fname = self.part_info.filename.as_deref().unwrap_or("");
        let clean_name = if raw_fname.is_empty() {
            let mut n = req.extract_filename();
            n.push_str(Server::get_ext_from_content_type(
                &self.part_info.content_type,
            ));
            n
        } else {
            sanitize_filename(raw_fname)
        };

        Some(self.path.join(clean_name))
    }

    fn trim_buffer(&mut self) {
        let b_len = self.boundary.len() + 4;
        if self.buffer.len() > b_len {
            let drain_to = self.buffer.len() - b_len;
            self.buffer.drain(..drain_to);
            self.current_pos = 0;
        }
    }

    fn save_file_part(&mut self, req: &HttpRequest, data_start: usize, data_end: usize) {
        let data = &self.buffer[data_start..data_end];
        let final_path = if let Some(path) = self.current_file_path.take() {
            path
        } else {
            let raw_fname = self.part_info.filename.as_deref().unwrap_or("");
            let clean_name = if raw_fname.is_empty() {
                let mut n = req.extract_filename();
                n.push_str(Server::get_ext_from_content_type(
                    &self.part_info.content_type,
                ));
                n
            } else {
                sanitize_filename(raw_fname)
            };
            get_unique_path(&self.path, &clean_name)
        };

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&final_path)
        {
            if file.write_all(data).is_ok() {
                self.files_saved += 1;
                self.saved_filenames.push(
                    final_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }

        self.current_file_path = None;
    }
}

/*

if files_saved > 0 {
            let mut res = HttpResponse::new(HTTP_CREATED, "Created");
            if files_saved == 1 {
                res.headers.insert(
                    "location".to_string(),
                    format!("/upload/{}", saved_filenames[0]),
                );
                return res.set_body(
                    format!("File saved as {}", saved_filenames[0]).into_bytes(),
                    "text/plain",
                );
            }
            let body_msg = format!("Saved files: {}", saved_filenames.join(", "));
            return res.set_body(body_msg.into_bytes(), "text/plain");
        }
        return Self::handle_error(HTTP_INTERNAL_SERVER_ERROR, Some(s_cfg));

        */
