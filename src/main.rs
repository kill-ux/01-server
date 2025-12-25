use mio::{Events, Poll};
use server_proxy::config::AppConfig;
use server_proxy::server::Server;
use server_proxy::http::Method;
use server_proxy::error::Result;

fn main() -> Result<()> {
    let config = AppConfig::parse()?;
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024); // Increased capacity for performance

    let mut server = Server::new(config, &poll)?;
    
    // Default routes (can be moved to a setup function)
    server.router.add_route(Method::GET, "/", |_| {
        let body = "HTTP/1.1 200 OK\r\nContent-Length: 12\r\n\r\nHello World!";
        body.as_bytes().to_vec()
    });

    println!("Server running. Monitoring {} listeners...", server.listeners.len());

    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            let token = event.token();

            if server.listeners.contains_key(&token) {
                if let Err(e) = server.handle_accept(&mut poll, token) {
                    eprintln!("Accept Error: {}", e);
                }
            } else {
                if let Err(e) = server.handle_connection(&poll, event, token) {
                    server.connections.remove(&token);
                }
            }
        }
    }
}