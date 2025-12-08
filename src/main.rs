use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};

use std::collections::HashMap;
use std::io::{self};
use std::net::Shutdown;
use std::time::Duration;

mod config;
mod http_processor; 
mod http_provider;
mod utils; 

use config::*;
use http_processor::*;
use http_provider::*;
use utils::sessions::*;

/// A listener entry; each server port has one instance
struct PortListener {
    host: String,
    listener: TcpListener,
    token: Token,
    port: u16,
    server_name: String,
}

fn main() -> io::Result<()> {
    //----------------------------------------
    // 1. Load and validate configuration
    //----------------------------------------
    let config = parses_configuration_file().expect("Invalid configuration");

    //----------------------------------------
    // 2. Setup Poll / Event loop
    //----------------------------------------
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(256);

    //----------------------------------------
    // 3. Setup listeners (multi-port)
    //----------------------------------------
    let mut listeners = Vec::new();

    let mut next_token_id = 0;

    for server in config.servers.iter() {
        for port in server.ports.iter() {
            let addr = format!("{}:{}", server.host, port).parse().unwrap();
            let mut listener = TcpListener::bind(addr)?;

            let token = Token(next_token_id);
            next_token_id += 1;

            poll.registry()
                .register(&mut listener, token, Interest::READABLE)?;
            
            listeners.push(PortListener {
                host: server.host.clone(),
                listener,
                token,
                port: *port,
                server_name: server.server_name.clone(),
            });

            println!("Listening on {}:{}", server.host, port);
        }
    }

    //----------------------------------------
    // 4. Create processor (route logic)
    //----------------------------------------
    let processor = HttpProcessor::new(config.clone());

    //----------------------------------------
    // 5. Active HTTP sessions
    //----------------------------------------
    let mut sessions: HashMap<Token, HttpSession> = HashMap::new();

    //----------------------------------------
    // 6. Event loop
    //----------------------------------------
    loop {
        poll.poll(&mut events, Some(Duration::from_millis(500)))?;

        for event in events.iter() {

            //----------------------------------------
            // A) New connections (listener socket)
            //----------------------------------------
            if let Some(listener) = listeners.iter_mut().find(|l| l.token == event.token()) {
                loop {
                    match listener.listener.accept() {
                        Ok((mut stream, _addr)) => {
                            let token = Token(next_token_id);
                            next_token_id += 1;

                            poll.registry()
                                .register(&mut stream, token, Interest::READABLE)?;

                            sessions.insert(token, HttpSession::new(stream, listener.port, listener.host.clone()));
                        }
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                        Err(err) => {
                            eprintln!("Accept error: {}", err);
                            break;
                        }
                    }
                }

                continue;
            }

            //----------------------------------------
            // B) Existing session readable
            //----------------------------------------
            if let Some(session) = sessions.get_mut(&event.token()) {
                match session.read_data() {
                    Ok(0) => {
                        // client closed
                        let _ = session.stream.shutdown(Shutdown::Both);
                        poll.registry().deregister(&mut session.stream)?;
                        sessions.remove(&event.token());
                        continue;
                    }
                    Ok(_) => {
                        // attempt to parse request
                        if let Some(request) = parse_http_request(&session.buffer) {
                            // Let processor choose route, root, etc.
                            let response = processor.process_request(&request, &session.port, &session.host);

                            let bytes = response.to_bytes();
                            let _ = session.write_response(&bytes);

                            // close connection
                            let _ = session.stream.shutdown(Shutdown::Both);
                            poll.registry().deregister(&mut session.stream)?;
                            sessions.remove(&event.token());
                        }
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(_) => {
                        sessions.remove(&event.token());
                        continue;
                    }
                }
            }
        }
    }
}