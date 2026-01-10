use crate::prelude::*;

pub struct Server {
    pub listeners: HashMap<Token, (TcpListener, Vec<Arc<ServerConfig>>)>,
    pub connections: HashMap<Token, HttpConnection>,
    pub cgi_to_client: HashMap<Token, Token>,
    pub next_token: usize,
    pub session_store: SessionStore,
}

impl Server {
    pub fn new(config: AppConfig, poll: &Poll) -> Result<Self> {
        let mut server = Self {
            listeners: HashMap::new(),
            connections: HashMap::new(),
            cgi_to_client: HashMap::new(),
            next_token: 0,
            session_store: SessionStore::new(10),
        };
        server.setup_listeners(config, &poll)?;
        Ok(server)
    }

    pub fn setup_listeners(&mut self, config: AppConfig, poll: &Poll) -> Result<()> {
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
            let token = Token(self.next_token);

            let mut listener = TcpListener::bind(addr)?;
            poll.registry()
                .register(&mut listener, token, Interest::READABLE)?;
            self.listeners.insert(token, (listener, config_list));

            self.next_token += 1;
        }

        Ok(())
    }

    pub fn run(&mut self, mut poll: Poll) -> Result<()> {
        let mut events = Events::with_capacity(1024);

        info!(
            "Server running. Monitoring {} listeners...",
            self.listeners.len()
        );

        loop {
            // Wait for events
            poll.poll(&mut events, Some(Duration::from_secs(1)))?;
            timeouts::process( self, &poll);

            for event in events.iter() {
                let token = event.token();

                if let Some(&client_token) = self.cgi_to_client.get(&token) {
                    if let Some(conn) = self.connections.get_mut(&client_token) {
                        if let Err(e) = handle_cgi_event(
                            &mut self.session_store,
                            &poll,
                            event,
                            token,
                            client_token,
                            conn,
                            &mut self.cgi_to_client,
                        ) {
                            eprintln!("Cgi Error: {}", e);
                            conn.closed = true;
                        }
                    }
                    continue;
                }

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
        // let conn = match self.connections.get_mut(&token) {
        //     Some(c) => c,
        //     None => return Ok(()),
        // };
        if let Some(conn) = self.connections.get_mut(&token) {
            conn.touch();

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

                println!("we are read this bytes {}", conn.request.buffer.len());
                println!("our buffer cgi is => {}", conn.cgi_buffer.len());

                let mut interest = Interest::READABLE;
                if let ActiveAction::Cgi { .. } = conn.action
                    && conn.request.buffer.len() > MAX_READ_DATA
                {
                    interest = Interest::WRITABLE;
                    trace!(
                        "Backpressure: Buffer full ({}), pausing socket read",
                        conn.request.buffer.len()
                    );
                }

                poll.registry()
                    .reregister(&mut conn.stream, token, interest)?;

                if !conn.closed && !conn.request.buffer.is_empty() {
                    conn.closed = HttpRequest::proces_request(
                        poll,
                        token,
                        &mut self.next_token,
                        &mut self.cgi_to_client,
                        conn,
                        &mut self.session_store,
                    )?;
                }
            }

            if event.is_writable()
                && (!conn.write_buffer.is_empty()
                    || matches!(conn.action, ActiveAction::FileDownload(_, _)))
            {
                if conn.write_buffer.is_empty() {
                    if let ActiveAction::FileDownload(ref mut file, ref mut remaining) = conn.action
                    {
                        let mut chunk = vec![0u8; 8192]; // 8KB 
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

                conn.closed = conn.write_data() || conn.closed;
                if !conn.closed && conn.write_buffer.is_empty() {
                    let mut interest = Interest::READABLE;

                    if matches!(conn.action, ActiveAction::FileDownload(_, _)) {
                        interest |= Interest::WRITABLE;
                    }

                    poll.registry()
                        .reregister(&mut conn.stream, token, interest)?;

                    if !conn.request.buffer.is_empty()
                        && conn.request.state == ParsingState::RequestLine
                    {
                        info!(
                            "Write finished. Found leftover data in buffer, processing next request..."
                        );
                        // conn.response = HttpResponse::new(HTTP_OK, &HttpResponse::status_text(HTTP_OK));
                        conn.closed = HttpRequest::proces_request(
                            poll,
                            token,
                            &mut self.next_token,
                            &mut self.cgi_to_client,
                            conn,
                            &mut self.session_store,
                        )?;
                    }
                }
            }
            if conn.closed && conn.write_buffer.is_empty() && conn.cgi_buffer.is_empty() {
                // Borrow ends here, so we can remove safely below

                // conn.linger_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(10000));
            } else {
                return Ok(()); // Keep connection alive
            }
        }
        println!("remove connection");
        // self.connections.remove(&token);
        if let Some(mut conn) = self.connections.remove(&token) {
            if let ActiveAction::Cgi { child, .. } = &mut conn.action {
                let _ = child.kill(); // Ensure it stops
                let _ = child.wait(); // Reclaim process resources
                cleanup_cgi(&mut self.cgi_to_client, &mut conn);
            }
        }
        Ok(())
    }
}
