use crate::config::{AppConfig, RouteConfig, ServerConfig};
use crate::error::{CleanError, Result};
use crate::http::*;
use crate::router::{Router, RoutingError};
use mio::{
    Events, Interest, Poll, Token,
    event::Event,
    net::{TcpListener, TcpStream},
};
use proxy_log::{info, warn};
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
    pub port: u16,
}

impl HttpConnection {
    pub fn new(stream: TcpStream, addr: SocketAddr, config_idx: usize, port: u16) -> Self {
        Self {
            stream,
            addr,
            write_buffer: Vec::new(),
            request: HttpRequest::new(),
            config_idx,
            port,
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
    pub listener_to_servers: HashMap<Token, Vec<usize>>,
    next_token: usize,
}

impl Server {
    pub fn new(config: AppConfig, poll: &Poll) -> Result<Self> {
        let mut listeners = HashMap::new();
        let mut listener_to_servers = HashMap::new();
        let mut addr_to_tokens = HashMap::new();
        let mut next_token = 0;
        let mut router = Router::new();

        info!("Initializing server listeners...");

        for (s_idx, s_cfg) in config.servers.iter().enumerate() {
            for &port in &s_cfg.ports {
                let addr: SocketAddr = format!("{}:{}", s_cfg.host_header(), port).parse()?;

                // If this address (IP:Port) isn't bound yet, bind it
                let token = *addr_to_tokens.entry(addr).or_insert_with(|| {
                    let t = Token(next_token);
                    next_token += 1;
                    t
                });

                if !listeners.contains_key(&token) {
                    let mut listener = TcpListener::bind(addr)?;
                    poll.registry()
                        .register(&mut listener, token, Interest::READABLE)?;
                    listeners.insert(token, listener);
                }

                // Associate this specific server block index with this listener token
                listener_to_servers
                    .entry(token)
                    .or_insert_with(Vec::new)
                    .push(s_idx);

                // Populate Router: Map (Port + Host + Path) to the config
                for r_cfg in &s_cfg.routes {
                    router.add_route_config(
                        port,
                        &s_cfg.server_name,
                        &r_cfg.path,
                        Arc::new(r_cfg.clone()),
                    );

                    // If it's the default server for this port, also register the IP/Host-header
                    if s_cfg.default_server {
                        router.add_route_config(port, "_", &r_cfg.path, Arc::new(r_cfg.clone()));
                        router.add_route_config(
                            port,
                            &s_cfg.host_header(),
                            &r_cfg.path,
                            Arc::new(r_cfg.clone()),
                        );
                    }
                }
            }
        }

        Ok(Self {
            listeners,
            connections: HashMap::new(),
            router,
            listener_to_servers,
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
        // Get the list of possible servers for this listener
        let server_idxs = self
            .listener_to_servers
            .get(&token)
            .ok_or("Unknown listener")?;
        let port = self.listeners.get(&token).unwrap().local_addr()?.port();

        let listener = self.listeners.get_mut(&token).unwrap();
        loop {
            match listener.accept() {
                Ok((mut stream, addr)) => {
                    let client_token = Token(self.next_token);
                    self.next_token += 1;

                    poll.registry()
                        .register(&mut stream, client_token, Interest::READABLE)?;

                    // We store the PORT the client connected to
                    let mut conn = HttpConnection::new(stream, addr, server_idxs[0], port);
                    // conn.port = port; // You'll need to add 'port: u16' to HttpConnection struct
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
                    .and_then(|h| h.split(':').next())
                    .ok_or_else(|| CleanError::from("Missing Host header"))?;

                dbg!(host);

                let response = match router.resolve(conn.port, host, &request.url, &request.method)
                {
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
