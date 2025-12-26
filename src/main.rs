use mio::{Events, Poll};
use server_proxy::config::AppConfig;
use server_proxy::server::Server;
use server_proxy::http::*;
use server_proxy::error::Result;

fn main() -> Result<()> {
    let config = AppConfig::parse()?;
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(1024); // Increased capacity for performance

    let mut server = Server::new(config, &poll)?;
    
    // Default routes (can be moved to a setup function)
    server.router.add_route(Method::GET, "/", handle_index);

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
                    eprintln!("Error in connection: {}",e);
                    server.connections.remove(&token);
                }
            }
        }
    }
}


fn handle_index(req: &HttpRequest) -> HttpResponse {
    HttpResponse::new(200, "OK")
        .set_body(b"Welcome to the Home Page".to_vec(), "text/plain")
}