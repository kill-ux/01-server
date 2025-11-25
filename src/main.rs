// use std::collections::HashMap;
// use std::io::{Read, Write};
// use mio::net::TcpStream;



// // ============================================================================
// // Updated main.rs integration
// // ============================================================================

// use mio::net::{TcpListener, TcpStream};
// use mio::{Events, Interest, Poll, Token};
// use std::collections::HashMap;
// use std::io::{self, Write};
// use std::net::Shutdown;
// use std::time::Duration;

// fn main() -> io::Result<()> {
//     let config = parses_configuration_file().unwrap();
//     let mut poll = Poll::new()?;
//     let mut events = Events::with_capacity(128);
//     let mut listeners = Vec::new();

//     // Initialize the data provider and processor
//     let data_provider = DataProvider::new("./public"); // Your web root directory
//     let http_processor = HttpProcessor::new(data_provider);

//     // Create and register listeners (same as before)
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

//     let mut sessions: HashMap<Token, HttpSession> = HashMap::new();
//     let mut next_token = listeners.len();

//     loop {
//         poll.poll(&mut events, Some(Duration::from_millis(1000)))?;
        
//         for event in events.iter() {
//             if let Some(listener) = listeners.iter_mut().find(|l| l.token == event.token()) {
//                 accept_connections(
//                     &mut listener.listener,
//                     &mut poll,
//                     &mut sessions,
//                     &mut next_token,
//                     listener.port,
//                 )?;
//             } else if let Some(session) = sessions.get_mut(&event.token()) {
//                 match session.read_data() {
//                     Ok(0) => {
//                         // Connection closed
//                         poll.registry().deregister(&mut session.stream)?;
//                         sessions.remove(&event.token());
//                         continue;
//                     }
//                     Ok(_) => {
//                         // Try to parse the request
//                         if let Some(request) = parse_http_request(&session.buffer) {
//                             // Process the request
//                             let response = http_processor.process_request(&request);
//                             let response_bytes = response.to_bytes();
                            
//                             // Send response
//                             if session.write_response(&response_bytes).is_ok() {
//                                 let _ = session.stream.shutdown(Shutdown::Both);
//                             }
                            
//                             poll.registry().deregister(&mut session.stream)?;
//                             sessions.remove(&event.token());
//                         }
//                     }
//                     Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
//                     Err(_) => {
//                         sessions.remove(&event.token());
//                         continue;
//                     }
//                 }
//             }
//         }
//     }
// }


use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::Shutdown;
use std::time::Duration;
mod config;
use config::*;
use std::io::*;
mod http_processor;
use http_processor::*;

struct PortListener {
    listener: TcpListener,
    token: Token,
    name: String,
    port: u16,
}

struct Connection {
    stream: TcpStream,
    buf: Vec<u8>,
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

    let mut connections: HashMap<Token, Connection> = HashMap::new();
    let mut next_token = listeners.len(); // Start after server tokens

    loop {
        poll.poll(&mut events, Some(Duration::from_millis(1000)))?;
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
                if let Some(conn) = connections.get_mut(&event.token()) {
                    // Read from the stream
                    let mut tmp = [0u8; 4096];
                    match conn.stream.read(&mut tmp) {
                        // When the read returens 0 it means that the user did close the connect
                        Ok(0) => {
                            // connection closed
                            poll.registry().deregister(&mut conn.stream)?;
                            connections.remove(&event.token());
                            continue;
                        }
                        Ok(n) => conn.buf.extend_from_slice(&tmp[..n]),
                        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                        Err(_) => {
                            connections.remove(&event.token());
                            continue;
                        }
                    }

                    if let Some(request) = parse_http_request(&conn.buf) {
                        println!("Got headers:");
                        for (key, value) in &request.headers {
                            println!("{}: {}", key, value);
                        }

                        println!("Partial body bytes: {:?}", request.body);

                        
                        // Send response
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "Content-Length: 20\r\n",
                            "\r\n",
                            "<html>Hello World</html>"
                        )
                        .as_bytes();
                        let _ = conn.stream.write_all(response);
                        let _ = conn.stream.shutdown(Shutdown::Both);
                        poll.registry().deregister(&mut conn.stream)?;
                        connections.remove(&event.token());
                    }
                }
            }
        }
    }
}

fn accept_connections(
    server: &mut TcpListener,
    poll: &mut Poll,
    connections: &mut HashMap<Token, Connection>,
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
                connections.insert(
                    new_token,
                    Connection {
                        stream,
                        buf: Vec::new(),
                    },
                );

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
