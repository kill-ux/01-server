use crate::prelude::*;

pub struct Server {
    pub listeners: HashMap<Token, (TcpListener, Vec<Arc<ServerConfig>>)>,
    pub connections: HashMap<Token, HttpConnection>,
    pub cgi_to_client: HashMap<Token, Token>,
    pub next_token: usize,
    pub session_store: SessionStore,
    pub zombie_purgatory: Vec<Child>,
}

impl Server {
    pub fn new(config: AppConfig, poll: &Poll) -> Result<Self> {
        let mut server = Self {
            listeners: HashMap::new(),
            connections: HashMap::new(),
            cgi_to_client: HashMap::new(),
            next_token: 0,
            session_store: SessionStore::new(10),
            zombie_purgatory: Vec::new(),
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
            poll.poll(&mut events, Some(Duration::from_secs(1)))?;
            timeouts::process(self, &poll);

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

        let conn = match self.connections.get_mut(&token) {
            Some(c) => c,
            None => return Ok(()),
        };
        conn.touch();

        // PHASE 1: Handle Incoming Data
        if !conn.closed && event.is_readable() {
            HttpConnection::handle_read_phase(
                conn,
                poll,
                token,
                &mut self.next_token,
                &mut self.cgi_to_client,
                &mut self.session_store,
            )?;
        }

        // PHASE 2: Handle Outgoing Data
        if event.is_writable() {
            HttpConnection::handle_write_phase(
                conn,
                poll,
                token,
                &mut self.next_token,
                &mut self.cgi_to_client,
                &mut self.session_store,
            )?;
        }

        // PHASE 3: Connection Lifecycle (Keep-Alive or Close)
        if conn.should_close() {
            HttpConnection::terminate_connection(self, token);
        }

        Ok(())
    }
}
