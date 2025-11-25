// Session management

use mio::net::TcpStream;
use std::io::*;
use crate::http_processor::HttpRequest;

/// Represents an HTTP session with a client connection
pub struct HttpSession {
    pub stream: TcpStream,
    pub buffer: Vec<u8>,
    pub request: Option<HttpRequest>,
    pub keep_alive: bool,
}

impl HttpSession {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            keep_alive: false,
            request: None,
        }
    }

    /// Read data from the stream into the buffer
    pub fn read_data(&mut self) -> std::io::Result<usize> {
        let mut tmp = [0u8; 4096];
        match self.stream.read(&mut tmp) {
            Ok(0) => Ok(0),
            Ok(n) => {
                self.buffer.extend_from_slice(&tmp[..n]);
                Ok(n)
            }
            Err(e) => Err(e),
        }
    }

    /// Write response to the stream
    pub fn write_response(&mut self, response: &[u8]) -> std::io::Result<()> {
        self.stream.write_all(response)
    }

    /// Clear the buffer after processing
    pub fn clear_buffer(&mut self) {
        self.buffer.clear();
    }
}