use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::Shutdown;
use std::time::Duration;
mod config;
use config::*;
struct PortListener {
    listener: TcpListener,
    token: Token,
    name: String,
    port: u16,
}

fn main() -> io::Result<()> {
    let config = match parses_configuration_file() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);
    let mut listeners = Vec::new();

    // Create and register listeners
    for (i, server) in config.servers.iter().enumerate() {
        for port in server.ports.iter() {
            let address = format!("0.0.0.0:{}", port).parse().unwrap();
            let mut listener = TcpListener::bind(address)?;
            let token = Token(i);

            poll.registry()
                .register(&mut listener, token, Interest::READABLE)?;
            listeners.push(PortListener {
                listener,
                token,
                port: *port,
                name: server.server_name.to_owned(),
            });
        }
    }

    let mut connections = HashMap::new();
    let mut next_token = listeners.len(); // Start after server tokens

    loop {
        poll.poll(&mut events, Some(Duration::from_millis(1000)))?;
        dbg!(&events);
        for event in events.iter() {
            // Check if event is for one of our listeners
            if let Some(listener) = listeners.iter_mut().find(|l| l.token == event.token()) {
                accept_connections(
                    &mut listener.listener,
                    &mut poll,
                    &mut connections,
                    &mut next_token,
                    listener.port,
                )?;
            } else {
                // Handle client connections
                if let Some(stream) = connections.get_mut(&event.token()) {
                    let response = concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "Content-Length: 20\r\n",
                        "\r\n",
                        "<html>Hello World</html>"
                    )
                    .as_bytes();

                    match stream.write_all(response) {
                        Ok(_) => {
                            println!("Sent response to client");
                            // Keep connection open for more requests
                            let _ = stream.shutdown(Shutdown::Both);
                            poll.registry().deregister(stream)?;
                            connections.remove(&event.token());
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                        Err(_) => {
                            connections.remove(&event.token());
                        }
                    }
                    println!("// ... your existing connection handling code ...");
                }
            }
        }
    }
}

fn accept_connections(
    server: &mut TcpListener,
    poll: &mut Poll,
    connections: &mut HashMap<Token, TcpStream>,
    next_token: &mut usize,
    port: u16,
) -> io::Result<()> {
    loop {
        match server.accept() {
            Ok((mut stream, _)) => {
                println!("Accepted connection on port {}", port);
                let new_token = Token(*next_token);
                *next_token += 1;
                poll.registry().register(
                    &mut stream,
                    new_token,
                    Interest::READABLE.add(Interest::WRITABLE),
                )?;
                connections.insert(new_token, stream);
                // ... rest of accept logic same as before ...
            } // ... error handling same as before ...
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                println!("tow");
                return Ok(());
            }
            Err(e) => return Err(e),
            _ => {}
        }
    }
}
