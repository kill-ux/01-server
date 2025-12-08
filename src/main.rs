use server::config::*;

fn main() {
    let config = match parse_config() {
        Ok(cfg) => cfg,
        Err(err) => {
            println!("Error : {}", err);
            return;
        }
    };
    dbg!(config);
}
