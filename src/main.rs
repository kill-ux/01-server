use std::{collections::HashMap, error::Error};

use mio::{
    Events, Interest, Poll, Token,
    net::{TcpListener, TcpStream},
};

#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    GET,
    POST,
    DELETE,
    None,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub version: String, // "HTTP/1.1"
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub query_params: HashMap<String, String>,
    pub buffer: Vec<u8>,
}

impl HttpRequest {
    pub fn new() -> Self {
        HttpRequest {
            method: Method::None,
            url: String::new(),
            version: String::new(),
            headers: HashMap::new(),
            body: Vec::new(),
            query_params: HashMap::new(),
            buffer: Vec::new(),
        }
    }
}

const SERVER: Token = Token(0);
const CLIENT: Token = Token(1);

fn parse_request(request: HttpRequest, rows: &str) {
    let sep = "\r\n";
    let vec: Vec<&str> = rows.split("\r\n\r\n").collect();
    let request_line_and_headers = vec[0].sp;
    println!("{:?}", vec);
}

// -> Result<(), Box<dyn Error>>
fn main() {
    const HTTP_GET: &str = concat!(
        "GET /hello.htm HTTP/1.1\r\n",
        "Host: www.tutorialspoint.com\r\n",
        "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36\r\n",
        "Accept-Language: en-us\r\n",
        "Connection: Keep-Alive\r\n",
        "\r\n",
        "Hello"
    );

    const HTTP_POST: &str = concat!(
        "GET /hello.htm HTTP/1.1\r\n",
        "Host: www.tutorialspoint.com\r\n",
        "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36\r\n",
        "Accept-Language: en-us\r\n",
        "Connection: Keep-Alive\r\n",
        "\r\n"
    );

    let mut request = HttpRequest::new();
    let bytes = HTTP_GET.as_bytes();
    request.buffer.extend_from_slice(bytes);
    parse_request(request, HTTP_GET);
    // let mut poll = Poll::new()?;
    // let mut events = Events::with_capacity(128);

    // let addr = "127.0.0.1:13265".parse()?;
    // let mut server = TcpListener::bind(addr)?;

    // poll.registry().register(&mut server, SERVER, Interest::READABLE)?;

    // let mut client = TcpStream::connect(addr)?;
    // poll.registry().register(&mut client, CLIENT, Interest::READABLE | Interest::WRITABLE)?;

    // loop {
    //     poll.poll(&mut events, None)?;

    //     for event in events.iter() {
    //         match event.token() {
    //             SERVER => {
    //                 let con = server.accept()?;
    //                 drop(con);
    //             }

    //             CLIENT => {
    //                 if event.is_readable() {

    //                 }

    //                 if event.is_writable() {

    //                 }

    //                 return Ok(());
    //             }

    //             _ => unreachable!()
    //         }
    //     }
    // }
}


pub fn find_crlf(buffer: Vec<) ->  {

}