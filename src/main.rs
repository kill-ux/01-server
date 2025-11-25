use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};

use std::collections::HashMap;
use std::io::{self};
use std::net::Shutdown;
use std::time::Duration;

mod config; // your Config loader
mod http_processor; // your HttpProcessor (Per-route logic)
mod http_provider;
mod utils; // your HttpSession // your DataProvider (Files, CGI, etc.)

use config::*;
use http_processor::*;
use http_provider::*;
use utils::sessions::*;

/// A listener entry; each server port has one instance
struct PortListener {
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

                            sessions.insert(token, HttpSession::new(stream, listener.port));
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
                            let response = processor.process_request(&request, &session.port.to_string());

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

// use mio::net::{TcpListener, TcpStream};
// use mio::{Events, Interest, Poll, Token};
// use std::collections::HashMap;
// use std::io::{self, Write};
// use std::net::Shutdown;
// use std::time::Duration;
// mod config;
// use config::*;
// use std::io::*;
// mod http_processor;
// use http_processor::*;
// mod http_provider;
// use http_provider::*;

// struct PortListener {
//     listener: TcpListener,
//     token: Token,
//     name: String,
//     port: u16,
// }

// struct Connection {
//     stream: TcpStream,
//     buf: Vec<u8>,
// }

// fn main() -> io::Result<()> {
//     let config = match parses_configuration_file() {
//         Ok(cfg) => cfg,
//         Err(e) => {
//             eprintln!("Configuration error: {}", e);
//             std::process::exit(1);
//         }
//     };

//     let data_provider = DataProvider::new("./public"); // Your web root directory
//     let http_processor = HttpProcessor::new(data_provider);

//     let mut poll = Poll::new()?;
//     let mut events = Events::with_capacity(128);
//     let mut listeners = Vec::new();

//     // Create and register listeners
//     for (i, server) in config.servers.iter().enumerate() {
//         for port in server.ports.iter() {
//             let address = format!("0.0.0.0:{}", port).parse().unwrap();
//             let mut listener = TcpListener::bind(address)?;
//             let token = Token(i);

//             poll.registry()
//                 .register(&mut listener, token, Interest::READABLE)?;
//             listeners.push(PortListener {
//                 listener,
//                 token,
//                 port: *port,
//                 name: server.server_name.to_owned(),
//             });
//         }
//     }

//     let mut connections: HashMap<Token, Connection> = HashMap::new();
//     let mut next_token = listeners.len(); // Start after server tokens

//     loop {
//         poll.poll(&mut events, Some(Duration::from_millis(1000)))?;
//         for event in events.iter() {
//             // Check if event is for one of our listeners
//             if let Some(listener) = listeners.iter_mut().find(|l| l.token == event.token()) {
//                 accept_connections(
//                     &mut listener.listener,
//                     &mut poll,
//                     &mut connections,
//                     &mut next_token,
//                     listener.port,
//                 )?;
//             } else {
//                 if let Some(conn) = connections.get_mut(&event.token()) {
//                     // Read from the stream
//                     let mut tmp = [0u8; 4096];
//                     match conn.stream.read(&mut tmp) {
//                         // When the read returens 0 it means that the user did close the connect
//                         Ok(0) => {
//                             // connection closed
//                             poll.registry().deregister(&mut conn.stream)?;
//                             connections.remove(&event.token());
//                             continue;
//                         }
//                         Ok(n) => conn.buf.extend_from_slice(&tmp[..n]),
//                         Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
//                         Err(_) => {
//                             connections.remove(&event.token());
//                             continue;
//                         }
//                     }

//                     if let Some(request) = parse_http_request(&conn.buf) {
//                         println!("Got headers:");
//                         for (key, value) in &request.headers {
//                             println!("{}: {}", key, value);
//                         }

//                         println!("Partial body bytes: {:?}", request.body);

//                         // Send response
//                         let response = concat!(
//                             "HTTP/1.1 200 OK\r\n",
//                             "Content-Length: 20\r\n",
//                             "\r\n",
//                             "<html>Hello World</html>"
//                         )
//                         .as_bytes();
//                         let _ = conn.stream.write_all(response);
//                         let _ = conn.stream.shutdown(Shutdown::Both);
//                         poll.registry().deregister(&mut conn.stream)?;
//                         connections.remove(&event.token());
//                     }
//                 }
//             }
//         }
//     }
// }

// fn accept_connections(
//     server: &mut TcpListener,
//     poll: &mut Poll,
//     connections: &mut HashMap<Token, Connection>,
//     next_token: &mut usize,
//     port: u16,
// ) -> io::Result<()> {
//     loop {
//         match server.accept() {
//             Ok((mut stream, _)) => {
//                 println!("Accepted connection on port {}", port);
//                 let new_token = Token(*next_token);
//                 *next_token += 1;
//                 poll.registry().register(
//                     &mut stream,
//                     new_token,
//                     Interest::READABLE.add(Interest::WRITABLE),
//                 )?;
//                 connections.insert(
//                     new_token,
//                     Connection {
//                         stream,
//                         buf: Vec::new(),
//                     },
//                 );

//                 // ... rest of accept logic same as before ...
//             } // ... error handling same as before ...
//             Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
//                 println!("tow");
//                 return Ok(());
//             }
//             Err(e) => return Err(e),
//             _ => {}
//         }
//     }
// }
