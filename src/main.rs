use std::fs::read_to_string;

use mio::Poll;
use parser::{Parser, lexer::Tokenizer};
use server_proxy::{
    config::AppConfig,
    error::Result,
    http::{HttpRequest, HttpResponse},
    server::Server,
};

fn main() -> Result<()> {
    let config_content = read_to_string("config.yaml")?;
    let tokenizer = Tokenizer::new(&config_content);
    // dbg!(tokenizer.tokenize());
    let mut parser = Parser::new(tokenizer).unwrap();
    let res = parser.parse_all();
    dbg!(res);

    return Ok(());

    // 1. Initialization
    let config = AppConfig::parse()?;
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
