use mio::Poll;
use server_proxy::{
    config::AppConfig,
    error::Result,
    http::{HttpRequest, HttpResponse, Method},
    server::Server,
};

fn main() -> Result<()> {
    // 1. Initialization
    let config = AppConfig::parse()?;
    let poll = Poll::new()?;

    // 2. Setup Server & Routes
    let mut server = Server::new(config, &poll)?;

    server
        .router
        .add_route(Method::GET, "default", "/", handle_index);
    server
        .router
        .add_route(Method::GET, "default", "/api", handle_api);

    // 3. Start the Engine
    server.run(poll)
}

// Handlers stay clean and isolated
fn handle_index(_req: &HttpRequest) -> HttpResponse {
    HttpResponse::new(200, "OK").set_body(b"<h1>Welcome to Home</h1>".to_vec(), "text/html")
}

fn handle_api(req: &HttpRequest) -> HttpResponse {
    HttpResponse::new(200, "OK")
        .set_header("Access-Control-Allow-Origin", "*")
        .set_body(b"{\"status\": \"active\"}".to_vec(), "application/json")
}
