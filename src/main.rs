use mio::{
    Events, Interest, Poll, Token,
    net::{TcpListener, TcpStream},
};
use server_proxy::error::Result;
use std::{
    collections::HashMap,
    fmt,
    io::{Read, Write},
    net::SocketAddr,
};


pub struct HttpConnection {
    conn: TcpStream,
    addr: SocketAddr,
}

pub struct Server {
    listener: TcpListener,
    connections: HashMap<Token, HttpConnection>,
    next_token: usize,
}

impl Server {
    const SERVER: Token = Token(1);

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

    let addr: SocketAddr = "127.0.0.1:13265".parse()?;
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
                    if let Ok((mut conn, addr)) = server.listener.accept() {
                        let token = server.next_token();
                        poll.registry().register(
                            &mut conn,
                            token,
                            Interest::READABLE | Interest::WRITABLE,
                        )?;
                        server
                            .connections
                            .insert(token, HttpConnection { conn, addr });
                        println!("New connection accepted");
                    }
                }

                token_client => {
                    println!("client token is => {:?}", token_client);
                    if let Some(c) = server.connections.get_mut(&token_client) {
                        if event.is_readable() {
                            let mut buf = String::new();
                            let res = c.conn.read_to_string(&mut buf);
                            println!("bytes {}", buf)
                        }

                        if event.is_writable() {
                            let res = c.conn.write("Hello".as_bytes());
                        }
                    }
                }
            }
        }
    }
}
