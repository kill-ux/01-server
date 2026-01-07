#[cfg(test)]
mod integration_tests {
    use mio::Poll;
    use server_proxy::config::{AppConfig, RouteConfig, ServerConfig};
    use server_proxy::http::Method;
    use server_proxy::server::Server;
    use std::error::Error;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
    use std::thread::sleep;
    use std::time::Duration;
    use std::{fs, thread};

    #[test]
    fn test_server_chunked_processing() {
        // --- 1. PREPARE DIRECTORY STRUCTURE ---
        // We use a specific folder for this test to avoid messing with your project
        let test_root = "./tmp_test_root";
        let upload_path = "./tmp_test_root/uploads";

        // Clean up any old test data and create fresh folders
        let _ = fs::remove_dir_all(test_root);
        fs::create_dir_all(upload_path).expect("Failed to create test directories");

        // --- 2. SETUP APP CONFIGURATION ---
        let mut config = AppConfig::default();

        let mut router1 = RouteConfig::default();
        router1.path = "/upload".to_string();
        router1.root = test_root.to_string();
        router1.upload_dir = "uploads".to_string(); // Files go to ./tmp_test_root/uploads
        router1.methods = vec![Method::POST.to_string(), Method::GET.to_string()];

        let server_cfg = ServerConfig {
            server_name: "127.0.0.1".to_string(),
            ports: vec![8080],
            root: test_root.to_string(),
            routes: vec![router1],
            default_server: true,
            client_max_body_size: 1024 * 1024, // 1MB
            ..Default::default()
        };
        config.servers.push(server_cfg);

        // --- 3. START SERVER IN BACKGROUND ---
        thread::spawn(move || {
            let poll = Poll::new().unwrap();
            let mut server = Server::new(config, &poll).unwrap();
            println!("Test Server starting...");
            server.run(poll).unwrap();
        });

        // Give the server time to bind the port
        thread::sleep(Duration::from_millis(300));

        // --- 4. CONNECT AND SEND CHUNKED REQUEST ---
        let mut stream = TcpStream::connect("127.0.0.1:8080").expect("Failed to connect to server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        let headers = "POST /upload/test.txt HTTP/1.1\r\n\
                       Host: 127.0.0.1:8080\r\n\
                       Transfer-Encoding: chunked\r\n\
                       Content-Type: text/plain\r\n\r\n";

        println!("Client: Sending headers...");
        stream.write_all(headers.as_bytes()).unwrap();

        println!("Client: Sending chunk 1...");
        stream.write_all(b"5\r\nHello\r\n").unwrap();
        thread::sleep(Duration::from_millis(100)); // Force server to handle partial data

        println!("Client: Sending chunk 2...");
        stream.write_all(b"7\r\n World!\r\n").unwrap();
        thread::sleep(Duration::from_millis(100));

        println!("Client: Sending terminator...");
        stream.write_all(b"0\r\n\r\n").unwrap();
        stream.flush().unwrap();

        // --- 5. READ RESPONSE ---
        let mut buffer = [0u8; 4096];
        match stream.read(&mut buffer) {
            Ok(n) => {
                let response = String::from_utf8_lossy(&buffer[..n]);
                println!("Server Response:\n{}", response);

                // Assert HTTP Success
                assert!(response.contains("201 Created") || response.contains("200 OK"));
            }
            Err(e) => panic!("Failed to read response from server: {}", e),
        }

        // --- 6. VERIFY FILE ON DISK ---
        // We look inside the upload_path for a file containing our text
        let paths = fs::read_dir(upload_path).unwrap();
        let mut found_content = false;

        for path in paths {
            let file_path = path.unwrap().path();
            if file_path.is_file() {
                let content = fs::read_to_string(&file_path).unwrap();
                if content == "Hello World!" {
                    found_content = true;
                    println!("Verified: File {:?} contains correct data.", file_path);
                }
            }
        }

        assert!(
            found_content,
            "Chunked data was not correctly assembled on disk."
        );

        // --- 7. CLEANUP ---
        let _ = fs::remove_dir_all(test_root);
        println!("Test finished and cleaned up.");
    }

    #[test]
    fn test_pipelined_requests() {
        let test_root = "./tmp_pipeline_test";
        let _ = fs::remove_dir_all(test_root); // Clean start
        fs::create_dir_all(test_root).unwrap();
        fs::write(format!("{}/index.html", test_root), "Hello").unwrap();

        let mut config = AppConfig::default();
        let mut router1 = RouteConfig::default();

        // FIX 1: Set path to "/" so "/index.html" is found in test_root
        router1.path = "/".to_string();
        router1.root = test_root.to_string();
        router1.methods = vec!["GET".to_string()];

        let server_cfg = ServerConfig {
            server_name: "localhost".to_string(), // Match the Host header in pipeline_data
            ports: vec![8081], // Use a different port than the chunked test to avoid conflicts
            root: test_root.to_string(),
            routes: vec![router1],
            default_server: true,
            ..Default::default()
        };
        config.servers.push(server_cfg);

        thread::spawn(move || {
            let poll = Poll::new().unwrap();
            let mut server = Server::new(config, &poll).unwrap();
            server.run(poll).unwrap();
        });

        thread::sleep(Duration::from_millis(300));

        let mut stream = TcpStream::connect("127.0.0.1:8081").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();

        // 3. Send TWO requests
        let pipeline_data = "GET /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n\
                             GET /index.html HTTP/1.1\r\nHost: localhost\r\n\r\n";

        stream.write_all(pipeline_data.as_bytes()).unwrap();

        // 4. Read Response 1
        let mut buffer = [0u8; 4096];
        let n1 = stream.read(&mut buffer).unwrap();
        let res1 = String::from_utf8_lossy(&buffer[..n1]);
        println!("Received First Buffer:\n{}", res1);

        // Check for 200 OK (or 404 if pathing is still off, to see what happened)
        assert!(
            res1.contains("200 OK"),
            "First response was not 200 OK. Check server logs."
        );

        // 5. Read Response 2
        // If Response 2 isn't in the first read, read again.
        if res1.matches("HTTP/1.1").count() < 2 {
            let n2 = stream.read(&mut buffer).unwrap();
            let res2 = String::from_utf8_lossy(&buffer[..n2]);
            println!("Received Second Buffer:\n{}", res2);
            assert!(res2.contains("200 OK"), "Second response was not 200 OK");
        }

        let _ = fs::remove_dir_all(test_root);
    }

    #[test]
    fn test_streaming_chunked_upload() -> Result<(), Box<dyn Error>> {
        let addr = "127.0.0.1:8080";
        let mut stream = TcpStream::connect(addr)?;
        println!("Connected to {}", addr);

        // 1. Send Headers
        let headers = "POST /upload HTTP/1.1\r\n\
                   Host: localhost\r\n\
                   Transfer-Encoding: chunked\r\n\
                   Content-Type: text/plain\r\n\r\n";
        stream.write_all(headers.as_bytes())?;
        stream.flush()?;
        println!("Headers sent. Sleeping...");
        sleep(Duration::from_millis(500));

        // 2. Send only the FIRST SIZE line
        stream.write_all(b"B\r\n")?; // Hex B = 11 bytes
        stream.flush()?;
        println!("First chunk size sent. Sleeping (Server is now in ReadData state)...");
        sleep(Duration::from_millis(500));

        // 3. Send the FIRST DATA
        stream.write_all(b"Rust Stream")?;
        stream.write_all(b"\r\n")?; // Trailing CRLF
        stream.flush()?;
        println!("First chunk data sent.");
        sleep(Duration::from_millis(500));

        // 4. Send the TERMINAL chunk (Size 0)
        // We split the "0\r\n\r\n" to see if the server waits correctly
        stream.write_all(b"0\r\n")?;
        stream.flush()?;
        println!("Terminal size sent. Sleeping before final CRLF...");
        sleep(Duration::from_millis(500));

        stream.write_all(b"\r\n")?;
        stream.flush()?;
        println!("Request finished.");

        // Stay alive for a second to read the response
        sleep(Duration::from_secs(1));
        Ok(())
    }
}
