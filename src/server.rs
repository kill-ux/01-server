use crate::config::{AppConfig, RouteConfig};
use crate::error::Result;
use crate::http::*;
use crate::router::{Router, RoutingError};
use mio::{
    Events, Interest, Poll, Token,
    event::Event,
    net::{TcpListener, TcpStream},
};
use tracing::info;
use std::collections::{HashMap, HashSet};
use std::io::{ErrorKind, Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;

const READ_BUF_SIZE: usize = 4096;

pub struct HttpConnection {
    pub stream: TcpStream,
    pub addr: SocketAddr,
    pub write_buffer: Vec<u8>,
    pub request: HttpRequest,
    pub config_idx: usize,
}

impl HttpConnection {
    pub fn new(stream: TcpStream, addr: SocketAddr, config_idx: usize) -> Self {
        Self {
            stream,
            addr,
            write_buffer: Vec::new(),
            request: HttpRequest::new(),
            config_idx,
        }
    }
}
impl HttpConnection {
    // Returns true if the connection should be closed
    fn read_data(&mut self, max_body_size: usize) -> core::result::Result<bool, ParseError> {
        let mut buf = [0u8; READ_BUF_SIZE]; // READ_BUF_SIZE
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => return Ok(true), // EOF
                Ok(n) => {
                    let absolute_limit: usize = 16_384 + max_body_size;
                    if self.request.buffer.len() + n > absolute_limit {
                        return Err(ParseError::PayloadTooLarge);
                    }
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
    pub listeners: HashMap<Token, TcpListener>,
    pub connections: HashMap<Token, HttpConnection>,
    pub router: Router,
    pub config: AppConfig,
    pub listener_to_config: HashMap<Token, usize>,
    next_token: usize,
}

impl Server {
    pub fn new(config: AppConfig, poll: &Poll) -> Result<Self> {
        let mut listeners = HashMap::new();
        let mut listener_to_config = HashMap::new();
        let mut bound_ports = std::collections::HashSet::new();
        let mut next_token = 0;
        let mut router = Router::new();

        let mut default_check = HashSet::new();
        let mut name_port_check = HashSet::new();

        info!("Setting up server listeners...");

        for (config_idx, s_cfg) in config.servers.iter().enumerate() {
            // 1. Fill Router first
            for r_cfg in &s_cfg.routes {
                let shared_cfg = Arc::new(r_cfg.clone());

                if !s_cfg.server_name.is_empty() {
                    router.add_route_config(
                        &s_cfg.server_name,
                        &r_cfg.path,
                        Arc::clone(&shared_cfg),
                    );
                }

                if s_cfg.default_server || s_cfg.server_name.is_empty() {
                    router.add_route_config(&s_cfg.host, &r_cfg.path, Arc::clone(&shared_cfg));
                }
            }

            for &port in &s_cfg.ports {
                let name = if s_cfg.server_name.is_empty() {
                    "_"
                } else {
                    &s_cfg.server_name
                };

                let identifier = format!("{}:{}", name, port);
                
                if !name_port_check.insert(identifier.clone()) {
                    return Err(format!(
                        "Conflict: server_name '{}' is already defined on port {}",
                        name, port
                    )
                    .into());
                }

                let addr_str = format!("{}:{}", s_cfg.host, port);

                // Check: Is there already a default server for this specific IP:Port?
                if s_cfg.default_server {
                    if !default_check.insert(addr_str.clone()) {
                        return Err(
                            format!("Multiple default servers defined for {}", addr_str).into()
                        );
                    }
                }

                if !bound_ports.insert(addr_str.clone()) {
                    continue;
                }

                let addr: SocketAddr = addr_str.parse()?;
                let mut listener = TcpListener::bind(addr)?;
                let token = Token(next_token);

                poll.registry()
                    .register(&mut listener, token, Interest::READABLE)?;

                listeners.insert(token, listener);
                listener_to_config.insert(token, config_idx);
                next_token += 1;
            }
        }

        Ok(Self {
            listeners,
            connections: HashMap::new(),
            router,
            listener_to_config,
            config,
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
        let config_idx = *self.listener_to_config.get(&token).unwrap();
        let listener = self.listeners.get_mut(&token).unwrap();

        loop {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    let client_token = Token(self.next_token);
                    self.next_token += 1;

                    poll.registry()
                        .register(&mut stream, client_token, Interest::READABLE)?;

                    self.connections
                        .insert(client_token, HttpConnection::new(stream, addr, config_idx));
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
            let s_cfg = &self.config.servers[conn.config_idx];
            if event.is_readable() {
                match conn.read_data(s_cfg.client_max_body_size) {
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

                    Self::process_static_request(poll, token, conn, &self.router)?;
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

    fn process_static_request(
        poll: &Poll,
        token: Token,
        conn: &mut HttpConnection,
        router: &Router,
    ) -> Result<()> {
        println!("### start prossing a request ###");
        while conn.request.parse_request().is_ok() {
            if conn.request.state == ParsingState::Complete {
                let request = &conn.request;
                let host = request
                    .headers
                    .get("Host")
                    .map(|h| h.split(":").next().unwrap_or(h))
                    .unwrap_or("default");

                dbg!(host);

                let response = match router.resolve(&request.method, host, &request.url) {
                    Ok(r_cfg) => {
                        if let Some(ref cgi_ext) = r_cfg.cgi_ext {
                            if request.url.ends_with(cgi_ext) {
                                Server::handle_cgi(request, r_cfg)
                            } else {
                                Server::handle_static_file(request, r_cfg)
                            }
                        } else {
                            Server::handle_static_file(request, r_cfg)
                        }
                    }
                    Err(RoutingError::MethodNotAllowed) => Router::method_not_allowed(),
                    Err(RoutingError::NotFound) => Router::not_found(),
                };

                println!("{}", conn.request);
                conn.write_buffer.extend_from_slice(&response.to_bytes());
                conn.request.finish_request();

                if conn.request.buffer.is_empty() {
                    break;
                }
            } else {
                break;
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

    pub fn handle_cgi(_request: &HttpRequest, _r_cfg: Arc<RouteConfig>) -> HttpResponse {
        HttpResponse::new(200, "OK").set_body(b"Hello World".to_vec(), "text/plain")
    }

    pub fn handle_static_file(_request: &HttpRequest, _r_cfg: Arc<RouteConfig>) -> HttpResponse {
        HttpResponse::new(200, "OK").set_body(_r_cfg.path.as_bytes().to_vec(), "text/plain")
    }
}
