use mio::Poll;
use parser::FromYaml;
use server_proxy::{
    config::AppConfig,
    error::Result,
    http::{HttpRequest, HttpResponse},
    server::Server,
};

fn main() -> Result<()> {
    let content = std::fs::read_to_string("config.yaml")?;
    let config = AppConfig::from_str(&content)?;

    let poll = Poll::new()?;

    config.display_config();

    // 2. Setup Server & Routes
    let mut server = Server::new(config, &poll)?;

    // 3. Start the Engine
    server.run(poll)
}

// Handlers stay clean and isolated
fn _handle_index(_req: &HttpRequest) -> HttpResponse {
    HttpResponse::new(200, "OK").set_body(b"<h1>Welcome to Home</h1>".to_vec(), "text/html")
}

fn _handle_api(_req: &HttpRequest) -> HttpResponse {
    HttpResponse::new(200, "OK")
        .set_header("Access-Control-Allow-Origin", "*")
        .set_body(b"{\"status\": \"active\"}".to_vec(), "application/json")
}
