#[cfg(test)]
mod integration_tests {
    use mio::Poll;
    use server_proxy::config::{AppConfig, RouteConfig, ServerConfig};
    use server_proxy::http::Method;
    use server_proxy::server::Server;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::Path;
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
}
