use mio::Poll;
use parser::FromYaml;
use server_proxy::{config::AppConfig, error::Result, server::Server};

fn main() -> Result<()> {
    let content = std::fs::read_to_string("config.yaml")?;
    let mut config = AppConfig::from_str(&content)?;
    config.validate()?;
    config.display_config();

    let poll = Poll::new()?;
    let mut server = Server::new(config, &poll)?;
    server.run(poll)
}
