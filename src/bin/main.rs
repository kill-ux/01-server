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
    let mut config = AppConfig::from_str(&content)?;
    config.validate()?;
    config.display_config();
    let mut server = Server::new(config)?;
    server.run()
}