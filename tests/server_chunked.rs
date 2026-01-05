#[cfg(test)]
mod integration_tests {
    use mio::Poll;
    use server_proxy::config::{AppConfig, ServerConfig};
    use server_proxy::server::Server;

    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use std::time::Duration;
    use std::{fs, thread};

    #[test]
    fn test_server_chunked_processing() {
        // 1. Setup a dummy AppConfig (Modify this to match your actual config struct)
        // Ensure you have a route that points to a valid 'root' and 'upload_dir'
        let mut config = AppConfig::default();

        let server_cfg = ServerConfig {
            server_name: "127.0.0.1".to_string(),
            ports: vec![8080],
            root: "./tmp_test_root".to_string(),
            ..Default::default()
        };
        config.servers.push(server_cfg);

        // 2. Start the server in a separate thread
        thread::spawn(move || {
            let poll = Poll::new().unwrap();
            let mut server = Server::new(config, &poll).unwrap();
            server.run(poll).unwrap();
        });

        // Give the server a moment to bind to the port
        thread::sleep(Duration::from_millis(100));

        // 3. Connect as a client
        let mut stream = TcpStream::connect("127.0.0.1:8080").expect("Failed to connect");
        stream.set_nonblocking(false).unwrap();

        // 4. Send Headers
        let headers = "POST /upload HTTP/1.1\r\n\
                       Host: 127.0.0.1:8080\r\n\
                       Transfer-Encoding: chunked\r\n\
                       Content-Type: text/plain\r\n\r\n";
        stream.write_all(headers.as_bytes()).unwrap();
        thread::sleep(Duration::from_millis(50)); // Simulate network delay

        // 5. Send Chunk 1: "Hello" (size 5)
        stream.write_all(b"5\r\nHello\r\n").unwrap();
        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(50));

        // 6. Send Chunk 2: " World" (size 6)
        stream.write_all(b"6\r\n World\r\n").unwrap();
        stream.flush().unwrap();
        thread::sleep(Duration::from_millis(50));

        // 7. Send Terminator: "0"
        stream.write_all(b"0\r\n\r\n").unwrap();
        stream.flush().unwrap();

        // 8. Read Response
        let mut buffer = [0u8; 4096];
        let n = stream.read(&mut buffer).expect("Failed to read response");
        let response = String::from_utf8_lossy(&buffer[..n]);
        println!("Response: {}", response);
        // 9. Assertions
        assert!(response.contains("201 Created") || response.contains("200 OK"));

        // Check if file exists on disk (if your handle_upload is working)
        let saved_file = Path::new("path/to/upload/test.txt");
        assert!(saved_file.exists());

        let mut buffer = [0u8; 4096];
        let n = stream.read(&mut buffer).expect("Failed to read response");
        let response = String::from_utf8_lossy(&buffer[..n]);
        println!("Response: {}", response);
        // Cleanup
        fs::remove_file(saved_file).ok();
    }
}
