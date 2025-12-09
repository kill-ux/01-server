use server::config::*;

fn main() {
    let config = match Config::parse() {
        Ok(cfg) => cfg,
        Err(err) => {
            println!("Error : {}", err);
            return;
        }
    };
    dbg!(config);

    for server in config.servers {
        
    }

    println!("Hello World");
}
