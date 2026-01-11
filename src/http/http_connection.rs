use crate::prelude::*;

#[derive(Debug)]
pub struct HttpConnection {
    pub stream: TcpStream,
    pub write_buffer: Vec<u8>,
    pub request: HttpRequest,
    pub response: HttpResponse,
    pub config_list: Vec<Arc<ServerConfig>>,
    pub s_cfg: Option<Arc<ServerConfig>>,
    pub action: ActiveAction,
    pub upload_manager: Option<Upload>,
    pub total_body_read: usize,
    pub body_remaining: usize,
    pub boundary: String,
    pub closed: bool,
    pub linger_until: Option<Instant>,
    pub cgi_in_token: Option<Token>,
    pub cgi_out_token: Option<Token>,
    pub cgi_buffer: Vec<u8>,
    pub session_id: Option<String>,
    pub last_activity: Instant,
}

#[derive(Debug)]
pub enum ActiveAction {
    Upload(PathBuf),
    FileDownload(File, usize),
    Cgi {
        out_stream: mio::net::UnixStream,
        in_stream: Option<mio::net::UnixStream>,
        child: std::process::Child,
        parse_state: CgiParsingState,
        header_buf: Vec<u8>,
        start_time: Instant,
    },
    Discard,
    None,
}

impl HttpConnection {
    pub fn new(stream: TcpStream, config_list: Vec<Arc<ServerConfig>>) -> Self {
        Self {
            stream,
            write_buffer: Vec::new(),
            request: HttpRequest::new(),
            response: HttpResponse::new(200, "OK"),
            upload_manager: None,
            config_list,
            s_cfg: None,
            action: ActiveAction::None,
            total_body_read: 0,
            body_remaining: 0,
            boundary: String::new(),
            closed: false,
            linger_until: None,
            cgi_in_token: None,
            cgi_out_token: None,
            cgi_buffer: Vec::new(),
            session_id: None,
            last_activity: Instant::now(),
        }
    }

    pub fn should_close(&self) -> bool {
        self.closed && self.write_buffer.is_empty() && self.cgi_buffer.is_empty()
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
    // Returns true if the connection should be closed
    pub fn read_data(&mut self) -> core::result::Result<bool, ParseError> {
        let mut buf = [0u8; READ_BUF_SIZE]; // READ_BUF_SIZE
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => return Ok(true), // EOF
                Ok(n) => {
                    self.request.buffer.extend_from_slice(&buf[..n]);
                    if self.request.buffer.len() >= MAX_READ_DATA / 2 {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => return Ok(true),
            }
        }
        Ok(false)
    }

    pub fn write_data(&mut self) -> bool {
        match self.stream.write(&self.write_buffer) {
            Ok(n) => {
                self.write_buffer.drain(..n);
                false
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => false,
            Err(_) => true,
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }
}

impl HttpConnection {
    /// Reads data from the client socket and dispatches it to the request parser.
    ///
    /// # Logic Steps
    /// 1. Drains the OS socket buffer into the `HttpConnection` request buffer.
    /// 2. Checks for EOF or read errors to update the `closed` state.
    /// 3. Implements CGI backpressure by switching interest to `WRITABLE` if the buffer is full.
    /// 4. Triggers `proces_request` if there is pending data to be parsed.
    pub fn handle_read_phase(
        conn: &mut HttpConnection,
        poll: &Poll,
        token: Token,
        next_token: &mut usize,
        cgi_to_client: &mut HashMap<Token, Token>,
        session_store: &mut SessionStore,
    ) -> Result<()> {
        match conn.read_data() {
            Ok(is_eof) => conn.closed = is_eof,
            Err(_) => conn.closed = true,
        }

        // Manage Backpressure for CGI
        let mut interest = Interest::READABLE;
        if matches!(conn.action, ActiveAction::Cgi { .. })
            && conn.request.buffer.len() > MAX_READ_DATA
        {
            interest = Interest::WRITABLE;
        }
        poll.registry()
            .reregister(&mut conn.stream, token, interest)?;

        // Process request if buffer has data
        if !conn.closed && !conn.request.buffer.is_empty() {
            conn.closed = HttpRequest::proces_request(
                poll,
                token,
                next_token,
                cgi_to_client,
                conn,
                session_store,
            )?;
        }
        Ok(())
    }

    /// Manages data egress by flushing buffers and handling file streaming.
    ///
    /// # Logic Steps
    /// 1. Refills the internal write buffer from an active file stream if currently empty.
    /// 2. Flushes the write buffer to the client socket and updates the connection's closed state.
    /// 3. If the buffer is fully drained and the connection is open, triggers post-write updates.
    /// 4. Supports HTTP Keep-Alive and Pipelining by checking for subsequent requests via `handle_post_write_update`.
    pub fn handle_write_phase(
        conn: &mut HttpConnection,
        poll: &Poll,
        token: Token,
        next_token: &mut usize,
        cgi_to_client: &mut HashMap<Token, Token>,
        session_store: &mut SessionStore,
    ) -> Result<()> {
        // 1. Fill buffer from file if needed
        if conn.write_buffer.is_empty() {
            if let ActiveAction::FileDownload(ref mut file, ref mut remaining) = conn.action {
                let mut chunk = vec![0u8; 8192];
                match file.read(&mut chunk) {
                    Ok(0) => conn.action = ActiveAction::None,
                    Ok(n) => {
                        conn.write_buffer.extend_from_slice(&chunk[..n]);
                        *remaining -= n;
                    }
                    Err(_) => conn.closed = true,
                }
            }
        }

        // 2. Flush buffer to socket
        if !conn.write_buffer.is_empty() {
            conn.closed = conn.write_data() || conn.closed;
        }

        // 3. Post-write logic: Check for pipelined requests or Keep-Alive
        if !conn.closed && conn.write_buffer.is_empty() {
            HttpConnection::handle_post_write_update(
                conn,
                poll,
                token,
                next_token,
                cgi_to_client,
                session_store,
            )?;
        }

        Ok(())
    }

    /// Updates connection state and handles pipelined requests after a successful write.
    ///
    /// # Logic Steps
    /// 1. Determines the next polling interest: defaults to `READABLE` but adds `WRITABLE`
    ///    if a file download is still in progress.
    /// 2. Resets the `HttpResponse` object to a clean state for the next request cycle.
    /// 3. Reregisters the connection with the system poller using the updated interest.
    /// 4. Implements HTTP Pipelining: if data remains in the request buffer, it immediately
    ///    triggers the parser for the next request.
    pub fn handle_post_write_update(
        conn: &mut HttpConnection,
        poll: &Poll,
        token: Token,
        next_token: &mut usize,
        cgi_to_client: &mut HashMap<Token, Token>,
        session_store: &mut SessionStore,
    ) -> Result<()> {
        let mut interest = Interest::READABLE;
        if matches!(conn.action, ActiveAction::FileDownload(_, _)) {
            interest |= Interest::WRITABLE;
        }

        conn.response = HttpResponse::new(HTTP_OK, &HttpResponse::status_text(HTTP_OK));
        poll.registry()
            .reregister(&mut conn.stream, token, interest)?;

        // PIPELINING
        if !conn.request.buffer.is_empty() && conn.request.state == ParsingState::RequestLine {
            
            info!("Write finished. Pipelined data detected, processing next request...");

            conn.closed = HttpRequest::proces_request(
                poll,
                token,
                next_token,
                cgi_to_client,
                conn,
                session_store,
            )?;
        }

        Ok(())
    }

    /// Cleans up a connection and its resources, specifically handling CGI process reaping.
    ///
    /// # Logic Steps
    /// 1. Removes the connection from the server's map.
    /// 2. Kills active CGI child processes and attempts to reap them.
    /// 3. Moves un-reaped processes to purgatory to prevent zombies.
    /// 4. Cleans up CGI-to-client internal mappings.
    pub fn terminate_connection(server: &mut Server, token: Token) {
        if let Some(mut conn) = server.connections.remove(&token) {
            println!("Removing connection: {:?}", token);
            let action = std::mem::replace(&mut conn.action, ActiveAction::None);

            if let ActiveAction::Cgi { mut child, .. } = action {
                let _ = child.kill();
                match child.try_wait() {
                    Ok(None) => server.zombie_purgatory.push(child),
                    _ => {} // Reaped
                }
                cleanup_cgi(&mut server.cgi_to_client, &mut conn);
            }
        }
    }
}
