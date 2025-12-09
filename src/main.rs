// // use mio::{Events, Interest, Poll, net::TcpListener};
// // use server_proxy::config::*;
// use server_proxy::error::Result;
// // use server_proxy::server::Token;
// use std::{io::{Read, Write}, net::{SocketAddr, TcpListener, TcpStream}};

// const BUFFER_SIZE: usize = 512;
// const ADDRESS: &str = "127.0.0.1:13265";

// fn handle_client(mut stream: TcpStream) {
//     println!("New connection from : {}", stream.peer_addr().unwrap());
//     let mut buffer = [0; BUFFER_SIZE];

//     loop {
//         match stream.read(&mut buffer) {
//             Ok(0) => {
//                 println!("Client disconnected");
//                 break;
//             }
//             Ok(bytes_read) => {
//                 let data = &buffer[..bytes_read];
//                 println!("Received {} bytes: {:?}", bytes_read, data);

//                 if stream.write_all(data).is_err() {
//                     println!("Failed to flush stream.");
//                     break;
//                 }
//             }
//             Err(e) => {
//                 eprintln!("An error occurred: {}", e);
//                 break;
//             }
//         }
//     }
// }

// fn main() -> Result<()> {
//     // let config =  Config::parse()?;
//     // let tokens: Token = Token::new();
//     // let mut poll = Poll::new()?;
//     // let mut events = Events::with_capacity(5);

//     // for server in &config.servers {
//     //     for port in &server.ports {
//     //         let addr: SocketAddr  =   format!("{}:{}",server.host, port).parse()?;
//     //         let mut listner = TcpListener::bind(addr)?;
//     //         poll.registry().register(&mut listner,mio::Token(tokens.next()) , Interest::READABLE)?;
//     //         dbg!(&addr);
//     //     }
//     // }

//     // loop {
//     //     poll.poll(&mut events, None)?;

//     //     for event in events.iter() {
//     //         match event.token() {

//     //         }
//     //     }
//     // }

//     let listner = TcpListener::bind(ADDRESS)?;
//     for con in listner.incoming() {
//         match con {
//             Ok(stream) => {
//                 dbg!(&stream);
//                 handle_client(stream);
//             }
//             Err(err) => {
//                 println!("{}",err)
//             }
//         }
//     }
//     Ok(())
// }
