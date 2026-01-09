use crate::prelude::*;

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
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
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
    Body,
    ChunkedBody,
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
    InvalidChunkSize,
    PayloadTooLarge,
    ParseHexError,
    Error(u16),
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
            ParseError::Error(_) => write!(f, "other error"),
            ParseError::InvalidChunkSize => write!(f, "invalid chunk size"),
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
pub enum ChunkState {
    ReadSize,
    ReadData(usize),
    ReadTrailingCRLF,
    ReadTrailers,
}

#[derive(Debug)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub version: String,
    pub headers: HashMap<String, String>,
    pub trailers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub body_file: Option<File>,
    pub is_large_body: bool,
    pub buffer: Vec<u8>,
    pub cursor: usize,
    pub state: ParsingState,
    pub chunk_state: ChunkState,
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
            trailers: HashMap::new(),
            body: Vec::new(),
            buffer: Vec::with_capacity(4096),
            cursor: 0,
            state: ParsingState::RequestLine,
            is_large_body: false,
            body_file: None,
            chunk_state: ChunkState::ReadSize,
        }
    }

    pub fn clear(&mut self) {
        self.state = ParsingState::RequestLine;
        self.headers.clear();
        self.trailers.clear();
        self.body.clear();
    }

    pub fn finish_request(&mut self) {
        self.buffer.drain(..self.cursor);
        self.cursor = 0;
        self.clear();
    }

    pub fn proces_request(
        poll: &Poll,
        token: Token,
        next_token: &mut usize,
        cgi_to_client: &mut HashMap<Token, Token>,
        conn: &mut HttpConnection,
    ) -> Result<bool> {
        let mut closed = false;
        // trace!("### start processing a request ###");
        loop {
            match HttpRequest::parse_request(conn, poll, next_token, cgi_to_client, token) {
                Ok(()) => {
                    trace!("### request state is complete ###");
                    let s_cfg = conn.s_cfg.as_ref().unwrap();

                    if let Some(upload_manager) = &mut conn.upload_manager {
                        let response = Upload::handel_upload_manager(upload_manager, s_cfg);
                        conn.write_buffer.extend_from_slice(&response.to_bytes());
                    }

                    conn.request.finish_request();
                    break;
                }
                Err(ParseError::IncompleteRequestLine) => break,
                Err(e) => {
                    let code = match e {
                        ParseError::PayloadTooLarge => HTTP_PAYLOAD_TOO_LARGE,
                        ParseError::InvalidMethod => HTTP_METHOD_NOT_ALLOWED,
                        ParseError::HeaderTooLong => HTTP_URI_TOO_LONG,
                        _ => HTTP_BAD_REQUEST,
                    };
                    let response = handle_error(code, conn.s_cfg.as_ref());
                    closed = true;
                    conn.write_buffer.extend_from_slice(&response.to_bytes());
                    conn.request.finish_request();
                    break;
                }
            }
        }

        if !conn.write_buffer.is_empty() || matches!(conn.action, ActiveAction::FileDownload(_, _))
        {
            poll.registry().reregister(
                &mut conn.stream,
                token,
                Interest::READABLE | Interest::WRITABLE,
            )?;
        }
        Ok(closed)
    }

    pub fn parse_request<'a>(
        conn: &mut HttpConnection,
        poll: &Poll,
        next_token: &mut usize,
        cgi_to_client: &mut HashMap<Token, Token>,
        client_token: Token,
    ) -> core::result::Result<(), ParseError> {
        loop {
            let res = match conn.request.state {
                ParsingState::RequestLine => conn.request.parse_request_line(),
                ParsingState::Headers => HttpRequest::parse_headers(conn),
                ParsingState::HeadersDone => {
                    if let Some(res) = HttpRequest::setup_action(
                        conn,
                        poll,
                        next_token,
                        cgi_to_client,
                        client_token,
                    )? {
                        conn.write_buffer.extend_from_slice(&res.to_bytes());
                        conn.request.state = ParsingState::Complete;
                    }
                    Ok(())
                }
                ParsingState::Body => HttpRequest::parse_unchunked_body(poll, conn),
                ParsingState::ChunkedBody => match HttpRequest::parse_chunked_body(conn) {
                    Ok(true) => {
                        conn.request.state = ParsingState::Complete;
                        Ok(())
                    }
                    Ok(false) => {
                        return Err(ParseError::IncompleteRequestLine);
                    }
                    Err(e) => Err(e),
                },
                _ => break,
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

    pub fn setup_action(
        conn: &mut HttpConnection,
        poll: &Poll,
        next_token: &mut usize,
        cgi_to_client: &mut HashMap<Token, Token>,
        client_token: Token,
    ) -> core::result::Result<Option<HttpResponse>, ParseError> {
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
            return Err(ParseError::PayloadTooLarge);
            // return Some(Server::handle_error(413, Some(&s_cfg)));
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
                    let program = match &r_cfg.cgi_path {
                        Some(p) => p.as_str(),
                        None => {
                            let ext = r_cfg.cgi_ext.as_deref().unwrap();
                            match ext {
                                "py" => "python3",
                                "sh" => "bash",
                                _ => "python3",
                            }
                        }
                    };

                    let full_script_path =
                        PathBuf::from(&s_cfg.root).join(request.url.trim_start_matches('/'));

                    // 1. Create the OUT pair (Script Output -> Server)
                    let Ok((server_out_std, script_out_std)) = UnixStream::pair() else {
                        return Ok(Some(handle_error(500, Some(&s_cfg))));
                    };
                    server_out_std.set_nonblocking(true).ok();
                    let mut server_out_mio = mio::net::UnixStream::from_std(server_out_std);

                    // 2. Setup Input pair (Server -> Script Input)
                    let Ok((server_in_std, script_in_std)) = UnixStream::pair() else {
                        return Ok(Some(handle_error(500, Some(&s_cfg))));
                    };
                    server_in_std.set_nonblocking(true).ok();
                    let mut server_in_mio = mio::net::UnixStream::from_std(server_in_std);

                    let script_output_file =
                        unsafe { File::from_raw_fd(script_out_std.into_raw_fd()) };
                    let script_input_file =
                        unsafe { File::from_raw_fd(script_in_std.into_raw_fd()) };

                    let mut cmd = Command::new(program);
                    cmd.arg(&full_script_path)
                        .envs(build_cgi_env(conn))
                        .stdin(Stdio::from(script_input_file))
                        .stdout(Stdio::from(script_output_file))
                        .stderr(Stdio::inherit());

                    match cmd.spawn() {
                        Ok(child) => {
                            let out_token = Token(*next_token);
                            *next_token += 1;
                            poll.registry()
                                .register(&mut server_out_mio, out_token, Interest::READABLE)
                                .ok();

                            let in_token = Token(*next_token);
                            *next_token += 1;
                            poll.registry()
                                .register(&mut server_in_mio, in_token, Interest::WRITABLE)
                                .ok();

                            conn.cgi_out_token = Some(out_token);
                            conn.cgi_in_token = Some(in_token);

                            conn.action = ActiveAction::Cgi {
                                out_stream: server_out_mio,
                                in_stream: Some(server_in_mio),
                                child,
                                parse_state: CgiParsingState::ReadHeaders,
                                header_buf: Vec::new(),
                            };

                            cgi_to_client.insert(out_token, client_token);
                            cgi_to_client.insert(in_token, client_token);

                            dbg!("cgi running");

                            None
                        }
                        Err(_) => Some(handle_error(500, Some(&s_cfg))),
                    }
                } else {
                    match request.method {
                        Method::GET => match handle_get(request, r_cfg, &s_cfg) {
                            (res, ActiveAction::FileDownload(file, file_size)) => {
                                conn.action = ActiveAction::FileDownload(file, file_size);
                                Some(res)
                            }
                            (res, _) => Some(res),
                        },
                        Method::POST => {
                            // Decide if we will upload to a file
                            if !r_cfg.upload_dir.is_empty() {
                                let path = PathBuf::from(&r_cfg.root).join(&r_cfg.upload_dir);
                                conn.action = ActiveAction::Upload(path);
                                None
                            } else {
                                Some(handle_error(HTTP_METHOD_NOT_ALLOWED, Some(&s_cfg)))
                            }
                        }
                        Method::DELETE => Some(handle_delete(request, r_cfg, &s_cfg)),
                    }
                }
            }
            Err(RoutingError::MethodNotAllowed) => {
                Some(handle_error(HTTP_METHOD_NOT_ALLOWED, Some(&s_cfg)))
            }
            Err(RoutingError::NotFound) => Some(handle_error(HTTP_NOT_FOUND, Some(&s_cfg))),
        };

        // 3. Update State based on body presence
        if res.is_none() {
            if is_chunked {
                conn.request.state = ParsingState::ChunkedBody;
            } else if content_length > 0 {
                conn.request.state = ParsingState::Body;
            } else {
                if matches!(conn.action, ActiveAction::Cgi { .. }) {
                    conn.request.state = ParsingState::Complete;
                } else {
                    return Ok(Some(HttpResponse::new(400, "Bad Request").set_body(
                        b"Error: No file data provided.".to_vec(),
                        "text/plain",
                    )));
                }
            }
        }

        dbg!(&res);
        dbg!(&conn.request.state);

        Ok(res)
    }

    fn parse_request_line(&mut self) -> core::result::Result<(), ParseError> {
        if let Some(abs_index) = find_crlf(&self.buffer, self.cursor) {
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
                    conn.request.buffer.drain(..conn.request.cursor);
                    conn.request.cursor = 0;
                    conn.request.state = ParsingState::HeadersDone;

                    return Ok(());
                }
            };
        }
    }

    pub fn parse_unchunked_body(
        poll: &Poll,
        conn: &mut HttpConnection,
    ) -> core::result::Result<(), ParseError> {
        if let Some(_) = &conn.s_cfg {
            let available = conn.request.buffer.len() - conn.request.cursor;
            let to_process = std::cmp::min(available, conn.body_remaining);
            // let cursor = conn.request.cursor;

            if to_process > 0 {
                match &mut conn.action {
                    ActiveAction::Cgi { in_stream, .. } => {
                        let data = conn.request.buffer.drain(..to_process).collect::<Vec<u8>>();
                        conn.cgi_buffer.extend_from_slice(&data);
                        conn.body_remaining -= to_process;

                        if let Some(in_token) = conn.cgi_in_token {
                            if let Some(pipe) = in_stream {
                                poll.registry()
                                    .reregister(pipe, in_token, Interest::WRITABLE)
                                    .ok();
                            }
                        }
                    }
                    _ => {
                        let start = conn.request.cursor;
                        execute_active_action(
                            &conn.request,
                            &mut conn.upload_manager,
                            &mut conn.action,
                            start,
                            to_process,
                            &conn.boundary,
                        )?;

                        conn.body_remaining -= to_process;
                        conn.request.buffer.drain(start..start + to_process);
                    }
                }
            }
        }

        if conn.body_remaining == 0 {
            conn.request.state = ParsingState::Complete;
        } else {
            return Err(ParseError::IncompleteRequestLine);
        }

        Ok(())
    }

    pub fn parse_chunked_body(conn: &mut HttpConnection) -> core::result::Result<bool, ParseError> {
        if let Some(s_cfg) = &conn.s_cfg {
            loop {
                match conn.request.chunk_state {
                    ChunkState::ReadSize => {
                        let current_len = conn.request.buffer.len();
                        if current_len == 0 {
                            return Ok(false);
                        }

                        let search_limit = std::cmp::min(current_len, 18);
                        match find_subsequence(&conn.request.buffer[..search_limit], b"\r\n", 0) {
                            Some(line_end) => {
                                let hex_str =
                                    String::from_utf8_lossy(&conn.request.buffer[..line_end]);
                                let chunk_size = usize::from_str_radix(hex_str.trim(), 16)
                                    .map_err(|_| ParseError::ParseHexError)?;
                                if conn.total_body_read + chunk_size > s_cfg.client_max_body_size {
                                    return Err(ParseError::PayloadTooLarge);
                                }

                                if chunk_size == 0 {
                                    // Check for the final \r\n after the 0
                                    if conn.request.buffer.len() < line_end + 2 {
                                        return Ok(false);
                                    }
                                    conn.request.buffer.drain(..line_end + 2); // Drain the "0\r\n"
                                    conn.request.chunk_state = ChunkState::ReadTrailers;
                                    continue;
                                }

                                conn.request.chunk_state = ChunkState::ReadData(chunk_size);
                                conn.request.buffer.drain(..line_end + 2);
                            }
                            None => {
                                if current_len >= 18 {
                                    return Err(ParseError::ParseHexError);
                                }
                                return Ok(false);
                            }
                        }
                    }

                    ChunkState::ReadData(remaining_size) => {
                        if conn.request.buffer.is_empty() {
                            return Ok(false);
                        }

                        let available = conn.request.buffer.len();
                        let to_read = std::cmp::min(available, remaining_size);

                        let data = conn.request.buffer.drain(..to_read).collect::<Vec<u8>>();

                        match &mut conn.action {
                            ActiveAction::Cgi { .. } => {
                                conn.cgi_buffer.extend_from_slice(&data);
                            }
                            _ => {
                                if let Some(mgr) = &mut conn.upload_manager {
                                    if !conn.boundary.is_empty() {
                                        mgr.upload_body_with_boundry(&conn.request, &data);
                                    } else {
                                        mgr.upload_simple_body(&conn.request, &data);
                                    }
                                }
                            }
                        }

                        conn.total_body_read += to_read;
                        let new_remaining = remaining_size - to_read;

                        if new_remaining == 0 {
                            conn.request.chunk_state = ChunkState::ReadTrailingCRLF;
                        } else {
                            conn.request.chunk_state = ChunkState::ReadData(new_remaining);
                            return Ok(false); // Yield to get more data from socket
                        }
                    }

                    ChunkState::ReadTrailingCRLF => {
                        if conn.request.buffer.len() < 2 {
                            return Ok(false); // Wait for the \r\n to arrive
                        }

                        if &conn.request.buffer[..2] != b"\r\n" {
                            return Err(ParseError::ParseHexError);
                        }
                        conn.request.buffer.drain(..2);
                        conn.request.chunk_state = ChunkState::ReadSize;
                    }

                    ChunkState::ReadTrailers => {
                        if conn.request.buffer.len() > 8192 {
                            // 8KB
                            return Err(ParseError::HeaderTooLong);
                        }
                        match conn.request.extract_and_parse_header() {
                            Ok(Some((k, v))) => {
                                if let Some(allowed_trailers) = conn.request.headers.get("trailer")
                                {
                                    if allowed_trailers.to_lowercase().contains(&k) {
                                        conn.request.trailers.insert(k, v);
                                    }
                                }
                                continue;
                            }
                            Ok(None) => {
                                conn.request.buffer.drain(..conn.request.cursor);
                                conn.request.cursor = 0;
                                return Ok(true);
                            }
                            Err(ParseError::IncompleteRequestLine) => return Ok(false),
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
        }
        Ok(true)
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

#[derive(Debug, Default)]
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
