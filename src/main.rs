use mio::{
    Events, Interest, Poll, Token,
    event::Event,
    net::{TcpListener, TcpStream},
};
use server_proxy::error::Result;
use std::{
    collections::HashMap,
    fmt,
    io::{Error, ErrorKind, Read, Write},
    net::SocketAddr,
};

pub struct HttpConnection {
    pub stream: TcpStream,
    pub addr: SocketAddr,
    pub read_buffer: Vec<u8>,
    pub write_buffer: Vec<u8>,
    // pub request: HttpRequest,
}

pub struct Server {
    listener: TcpListener,
    connections: HashMap<Token, HttpConnection>,
    next_token: usize,
}

impl Server {
    pub fn new(addr: SocketAddr) -> Result<Self> {
        let server = Server {
            listener: TcpListener::bind(addr)?,
            connections: HashMap::new(),
            next_token: 0,
        };

        Ok(server)
    }

    pub fn next_token(&mut self) -> Token {
        self.next_token += 1;
        return Token(self.next_token);
    }

    fn handle_accept_connections(&mut self, poll: &mut Poll) -> Result<()> {
        loop {
            match self.listener.accept() {
                Ok((mut stream, addr)) => {
                    let token = self.next_token();
                    poll.registry()
                        .register(&mut stream, token, Interest::READABLE)?;
                    self.connections.insert(
                        token,
                        HttpConnection {
                            stream,
                            addr,
                            read_buffer: Vec::with_capacity(4096),
                            write_buffer: Vec::new(),
                        },
                    );
                    println!("Accepted connection from {}", addr);
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => {
                    eprintln!("Accept error: {}", e);
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn handle_client_connection(
        &mut self,
        poll: &Poll,
        event: &Event,
        token_client: Token,
    ) -> Result<()> {
        let mut connection_closed = false;

        if let Some(conn) = self.connections.get_mut(&token_client) {
            // --- HANDLE READ ---
            if event.is_readable() {
                loop {
                    let mut temp_buf = [0u8; 1024];
                    match conn.stream.read(&mut temp_buf) {
                        Ok(0) => {
                            connection_closed = true;
                            break;
                        }
                        Ok(n) => {
                            conn.read_buffer.extend_from_slice(&temp_buf[..n]);
                        }
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => {
                            connection_closed = true;
                            break;
                        }
                    }
                }

                // ##########
                println!("{:?}", conn.read_buffer);
                conn.write_buffer.extend_from_slice(&conn.read_buffer);
                conn.read_buffer.clear();

                // change re
                poll.registry().reregister(
                    &mut conn.stream,
                    token_client,
                    Interest::READABLE | Interest::WRITABLE,
                )?;
            }

            // --- HANDLE WRITE ---
            if event.is_writable() && !conn.write_buffer.is_empty() {
                match conn.stream.write(&conn.write_buffer) {
                    Ok(n) => {
                        conn.write_buffer.drain(..n);
                        if conn.write_buffer.is_empty() {
                            poll.registry().reregister(
                                &mut conn.stream,
                                token_client,
                                Interest::READABLE,
                            )?;
                        }
                    }
                    Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(_) => connection_closed = true,
                }
            }

            // Check for HUP (Hang up)
            if event.is_read_closed() || event.is_write_closed() {
                connection_closed = true;
            }

            if connection_closed {
                println!("Closing connection for {:?}", token_client);
                self.connections.remove(&token_client);
            }
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    // let http_get = concat!("GET / HTTP/1.1\r\n", "Host: a.b.c\r\n", "\r\n", "Hello");

    // let mut request = HttpRequest::new();
    // request.buffer.extend_from_slice(http_get.as_bytes());

    // match parse_request(&mut request) {
    //     Ok(()) => println!("Parsed: {:?}", request),
    //     Err(e) => {
    //         eprintln!("Parse error: {}", e)
    //     }
    // }

    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);

    let addr: SocketAddr = "127.0.0.1:8080".parse()?;
    // let mut server = TcpListener::bind(addr)?;
    let mut server = Server::new(addr)?;

    const SERVER: Token = Token(0);

    poll.registry()
        .register(&mut server.listener, SERVER, Interest::READABLE)?;

    loop {
        poll.poll(&mut events, None)?;
        for event in events.iter() {
            match event.token() {
                SERVER => {
                    server.handle_accept_connections(&mut poll)?;
                }

                token_client => {
                    println!("client token is => {:?}", token_client);
                    server.handle_client_connection(&poll, event, token_client)?;
                }
            }
        }
    }
}
