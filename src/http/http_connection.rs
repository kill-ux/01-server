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

impl HttpConnection {
    // Returns true if the connection should be closed
    pub fn read_data(&mut self) -> core::result::Result<bool, ParseError> {
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
}
