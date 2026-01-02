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
    pub listener_to_servers: HashMap<Token, Vec<usize>>,
    next_token: usize,
}

impl Server {
    pub fn new(config: AppConfig, poll: &Poll) -> Result<Self> {
        let mut listeners = HashMap::new();
        let mut listener_to_servers = HashMap::new(); // Token -> Vec<usize> server_idxs
        let mut bound_ports = HashSet::new();
        let mut bound_addrs = HashSet::new();
        let mut next_token = 0;
        let mut router = Router::new();

        info!("Initializing server listeners and routing tables...");

        // Track unique bind addrs and multi-servers per addr
        let mut addr_to_servers: HashMap<SocketAddr, Vec<usize>> = HashMap::new();
        for (s_idx, s_cfg) in config.servers.iter().enumerate() {
            for &port in &s_cfg.ports {
                let addr: SocketAddr = format!("{}:{}", s_cfg.host_header(), port).parse()?;
                addr_to_servers.entry(addr).or_default().push(s_idx);
            }
        }

        for (addr, server_idxs) in addr_to_servers {
            let addr_str = addr.to_string();
            if !bound_addrs.insert(addr_str.clone()) {
                warn!("Skipping duplicate bind: {}", addr);
                continue;
            }

            let mut listener = TcpListener::bind(addr)?;
            let token = Token(next_token);

            poll.registry()
                .register(&mut listener, token, Interest::READABLE)?;

            listeners.insert(token, listener);
            listener_to_servers.insert(token, server_idxs); // Multi-server support
            next_token += 1;
            info!("Bound listener: {} -> servers {:?}", addr, server_idxs);
        }

        // Populate router ONLY for defaults/catch-alls (host/path, no port)
        for (s_idx, s_cfg) in config.servers.iter().enumerate() {
            let shared_cfg = Arc::new(s_cfg.clone());
            if s_cfg.default_server {
                for r_cfg in &s_cfg.routes {
                    let key = format!("{}/{}", s_cfg.host_header(), r_cfg.path);
                    router.add_route_config(&key, &r_cfg.path, Arc::clone(&shared_cfg));
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
                    .and_then(|h| h.split(':').next())
                    .ok_or_else(|| CleanError::from("Missing Host header"))?;

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

    pub fn catch_all(
        &self,
        listener_token: Token,
        method: &Method,
        host: Option<&str>,
        path: &str,
    ) -> core::result::Result<Arc<ServerConfig>, RoutingError> {
        // 1. Listener token → server candidates
        if let Some(server_idxs) = self.listener_to_servers.get(&listener_token) {
            // Exact server_name match
            if let Some(host) = host {
                for &s_idx in server_idxs {
                    let s_cfg = &self.config.servers[s_idx];
                    if s_cfg.server_name == host {
                        // Check routes within this server
                        if let Some(r_cfg) =
                            s_cfg.routes.iter().find(|r| r.path_matches(path, method))
                        {
                            return Ok(Arc::clone(&self.server_cfgs[s_idx]));
                        }
                    }
                }
            }
            // Fallback: first server's routes
            if let Some(&s_idx) = server_idxs.first() {
                let s_cfg = &self.config.servers[s_idx];
                if let Some(r_cfg) = s_cfg.routes.iter().find(|r| r.path_matches(path, method)) {
                    return Ok(Arc::clone(&self.server_cfgs[s_idx]));
                }
            }
        }

        // 2. Global router fallback (defaults)
        if let Some(host) = host {
            let key = format!("{}{}", host, path);
            if let Some(server_cfg) = self.routes.get(&key) {
                if method.is_allowed(&server_cfg.routes[0].methods) {
                    // Assume first route
                    return Ok(Arc::clone(server_cfg));
                }
            }
        }

        // Prefix fallback in global router
        let mut best_match: Option<&Arc<ServerConfig>> = None;
        for (prefix_key, server_cfg) in &self.routes {
            if prefix_key.starts_with(host.unwrap_or(""))
                && path.starts_with(&prefix_key[host.len_or_0()..])
            {
                if let Some(prev) = best_match {
                    if prefix_key.len() > prev_key(server_cfg).len() {
                        best_match = Some(server_cfg);
                    }
                } else {
                    best_match = Some(server_cfg);
                }
            }
        }

        best_match
            .ok_or(RoutingError::NotFound)
            .map(|cfg| Arc::clone(cfg))
    }
}
