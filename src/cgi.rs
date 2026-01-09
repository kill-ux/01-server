use crate::prelude::*;

#[derive(Debug, PartialEq)]
pub enum CgiParsingState {
    ReadHeaders,
    StreamBody,
    StreamBodyChuncked,
}

pub fn parse_cgi_headers(bytes: &[u8]) -> (u16, Vec<(String, String)>) {
    let mut status = 200;
    let mut headers = Vec::new();
    let content = String::from_utf8_lossy(bytes);

    for line in content.lines() {
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let val = val.trim().to_string();

            if key == "status" {
                status = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(200);
            } else {
                headers.push((key, val));
            }
        }
    }
    (status, headers)
}

pub fn parse_cgi_output(raw_output: &[u8]) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let mut header_end = 0;
    if let Some(pos) = find_subsequence(raw_output, b"\r\n\r\n", 0) {
        header_end = pos;
    }

    let header_section = String::from_utf8_lossy(&raw_output[..header_end]);
    let body = raw_output[header_end + 4..].to_vec();

    let mut status_code = 200;
    let mut headers = Vec::new();

    for line in header_section.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().to_string();

            if key == "status" {
                // CGI uses "Status: 404 Not Found", we just need the digits
                status_code = value
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(200);
            } else {
                headers.push((key, value));
            }
        }
    }

    (status_code, headers, body)
}

pub fn handle_cgi_event(
    poll: &Poll,
    event: &Event,
    cgi_token: Token,
    client_token: Token,
    conn: &mut HttpConnection,
    cgi_to_client: &mut HashMap<Token, Token>,
) -> Result<()> {
    dbg!(conn.body_remaining);
    dbg!(event.is_readable());
    dbg!(event.is_writable());
    if let ActiveAction::Cgi {
        out_stream,
        in_stream,
        child,
        parse_state,
        header_buf,
        start_time,
    } = &mut conn.action
    {
        if start_time.elapsed().as_secs() > 10 {
            dbg!("CGI TIMEOUT: Killing process");
        }
        // SCRIPT -> SERVER (Stdout)
        if event.is_readable() && Some(cgi_token) == conn.cgi_out_token {
            let mut buf = [0u8; 4096];
            match out_stream.read(&mut buf) {
                Ok(0) => {
                    if *parse_state == CgiParsingState::StreamBodyChuncked {
                        dbg!("write end");
                        conn.write_buffer.extend_from_slice(b"0\r\n\r\n");
                        poll.registry().reregister(
                            &mut conn.stream,
                            client_token,
                            Interest::READABLE | Interest::WRITABLE,
                        )?;
                    }
                    conn.cgi_out_token = None;
                    conn.cgi_in_token = None;
                    // conn.action = ActiveAction::None;
                    // conn.closed = true;
                }
                Ok(n) => {
                    dbg!("read from cgi a ", n);

                    // FIX: Pass individual fields instead of 'conn'
                    process_cgi_stdout(parse_state, header_buf, &mut conn.write_buffer, &buf[..n])?;

                    poll.registry().reregister(
                        &mut conn.stream,
                        client_token,
                        Interest::READABLE | Interest::WRITABLE,
                    )?;
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => conn.closed = true,
            }
        }

        dbg!(Some(cgi_token) == conn.cgi_in_token);

        // SERVER -> SCRIPT (Stdin)
        if event.is_writable() && Some(cgi_token) == conn.cgi_in_token {
            dbg!(!conn.cgi_buffer.is_empty());
            if !conn.cgi_buffer.is_empty() {
                dbg!("try write data");
                if let Some(pipe) = in_stream {
                    match pipe.write(&conn.cgi_buffer) {
                        Ok(n) => {
                            dbg!("write to cgi", n);
                            conn.cgi_buffer.drain(..n);

                            if conn.cgi_buffer.len() < 65536 {
                                poll.registry().reregister(
                                    &mut conn.stream,
                                    client_token,
                                    Interest::READABLE | Interest::WRITABLE,
                                )?;
                            }

                            if conn.body_remaining == 0 && conn.cgi_buffer.is_empty() {
                                conn.cgi_in_token = None;
                                trace!("CGI stdin pipe closed (EOF sent)");
                            }
                        }
                        Err(e) if e.kind() != ErrorKind::WouldBlock => {}
                        Err(_) => conn.closed = true,
                    }
                }
            }
        }

        // Child process status check
        dbg!("check status");
        match child.try_wait() {
            Ok(Some(_status)) => {
                dbg!("kill");
                if let ActiveAction::Cgi { in_stream, .. } = &mut conn.action {
                    if conn.body_remaining == 0 && conn.cgi_buffer.is_empty() {
                        if let Some(pipe) = in_stream.take() {
                            dbg!("SUCCESS: All bytes sent. Closing Pipe.");
                            drop(pipe);
                            conn.cgi_in_token = None;
                        }
                    }
                }

                cleanup_cgi(cgi_to_client, conn);
                conn.action = ActiveAction::None;
            }
            Ok(None) => {}
            Err(_) => conn.closed = true,
        }
    }
    Ok(())
}

pub fn build_cgi_env(conn: &mut HttpConnection) -> HashMap<String, String> {
    let req = &conn.request;
    let mut envs = HashMap::new();

    envs.insert("GATEWAY_INTERFACE".to_string(), "CGI/1.1".to_string());
    envs.insert("SERVER_PROTOCOL".to_string(), "HTTP/1.1".to_string());
    envs.insert("REQUEST_METHOD".to_string(), req.method.to_string());
    // envs.insert("QUERY_STRING".to_string(), req.query_string.clone());
    envs.insert("PATH_INFO".to_string(), req.url.clone());
    envs.insert("SCRIPT_NAME".to_string(), req.url.clone());

    envs.insert("SERVER_NAME".to_string(), "01-SERVER".to_string());
    if let Ok(addr) = conn.stream.peer_addr() {
        envs.insert("REMOTE_ADDR".to_string(), addr.ip().to_string());
        envs.insert("REMOTE_PORT".to_string(), addr.port().to_string());
    }

    if let Some(ct) = req.headers.get("content-type") {
        envs.insert("CONTENT_TYPE".to_string(), ct.clone());
    }
    if let Some(cl) = req.headers.get("content-length") {
        envs.insert("CONTENT_LENGTH".to_string(), cl.clone());
    }

    for (k, v) in req.headers.iter().chain(&req.trailers) {
        let env_key = format!("HTTP_{}", k.to_uppercase().replace('-', "_"));
        envs.insert(env_key, v.clone());
    }

    envs
}

pub fn process_cgi_stdout(
    parse_state: &mut CgiParsingState,
    header_buf: &mut Vec<u8>,
    write_buffer: &mut Vec<u8>,
    new_data: &[u8],
) -> Result<()> {
    match parse_state {
        CgiParsingState::ReadHeaders => {
            header_buf.extend_from_slice(new_data);

            if let Some(pos) = find_subsequence(header_buf, b"\r\n\r\n", 0)
                .or_else(|| find_subsequence(header_buf, b"\n\n", 0))
            {
                let is_crlf = header_buf.contains(&b'\r');
                let delimiter_len = if is_crlf { 4 } else { 2 };

                let header_bytes = header_buf[..pos].to_vec();
                let body_start = header_buf[pos + delimiter_len..].to_vec();

                let (status, cgi_headers) = parse_cgi_headers(&header_bytes);
                let mut res = HttpResponse::new(status, &HttpResponse::status_text(status));

                res.headers.remove("Content-Length");

                for (k, v) in cgi_headers {
                    res.set_header(&k, &v);
                }

                let is_chunked = !res.headers.contains_key("content-length");
                if is_chunked {
                    res.set_header("transfer-encoding", "chunked");
                    *parse_state = CgiParsingState::StreamBodyChuncked;
                } else {
                    *parse_state = CgiParsingState::StreamBody;
                }

                write_buffer.extend_from_slice(&res.to_bytes_headers_only());

                if !body_start.is_empty() {
                    push_cgi_data(write_buffer, &body_start, is_chunked);
                }
            }
        }
        CgiParsingState::StreamBody => {
            write_buffer.extend_from_slice(new_data);
        }
        CgiParsingState::StreamBodyChuncked => {
            push_cgi_data(write_buffer, new_data, true);
        }
    }
    Ok(())
}

fn push_cgi_data(write_buffer: &mut Vec<u8>, data: &[u8], chunked: bool) {
    if chunked {
        let header = format!("{:X}\r\n", data.len());
        write_buffer.extend_from_slice(header.as_bytes());
        write_buffer.extend_from_slice(data);
        write_buffer.extend_from_slice(b"\r\n");
    } else {
        write_buffer.extend_from_slice(data);
    }
}

pub fn cleanup_cgi(cgi_to_client: &mut HashMap<Token, Token>, conn: &mut HttpConnection) {
    if let Some(t) = conn.cgi_out_token.take() {
        cgi_to_client.remove(&t);
    }
    if let Some(t) = conn.cgi_in_token.take() {
        cgi_to_client.remove(&t);
    }
}

pub fn check_time_out_cgi(
    connections: &mut HashMap<Token, HttpConnection>,
    poll: &Poll,
    cgi_to_client: &mut HashMap<Token, Token>,
) {
    connections.retain(|token, conn| {
        if let ActiveAction::Cgi { start_time, .. } = &conn.action {
            if start_time.elapsed().as_secs() > TIMEOUT_CGI - 28 {
                println!("CRITICAL: CGI Process timed out (no events). Killing.");
                force_cgi_timeout(conn, cgi_to_client);

                poll.registry()
                    .reregister(&mut conn.stream, *token, Interest::WRITABLE)
                    .ok();
                return true;
            }
        }
        true
    });
}

pub fn force_cgi_timeout(conn: &mut HttpConnection, cgi_to_client: &mut HashMap<Token, Token>) {
    if let ActiveAction::Cgi { ref mut child, .. } = conn.action {
        // 1. Terminate the process
        let _ = child.kill();
        let _ = child.wait(); // Prevent zombie processes
        dbg!(&conn.action);

        if let ActiveAction::Cgi { parse_state, .. } = &conn.action {
            dbg!(&parse_state);
            if *parse_state == CgiParsingState::StreamBodyChuncked {
                let end_marker = "0\r\n\r\n";
                conn.write_buffer.extend_from_slice(end_marker.as_bytes());
            } else {
                // We haven't sent headers yet, so we can still send a 504 error
                let error_res = "HTTP/1.1 504 Gateway Timeout\r\nContent-Length: 0\r\n\r\n";
                conn.write_buffer.extend_from_slice(error_res.as_bytes());
            }
        }

        // if let Some(s_cfg) = &conn.s_cfg {
        //     let mut res = handle_error(504, Some(s_cfg));
        //     res.set_header("Connection", "close");
        //     conn.write_buffer.clear();
        //     conn.write_buffer.extend_from_slice(&res.to_bytes());
        // }

        // 3. Update connection state
        conn.cgi_in_token = None;
        conn.cgi_out_token = None;
        conn.cgi_buffer.clear();
        conn.closed = true; // Flag for removal after write

        // 4. Clean up the global CGI map
        cleanup_cgi(cgi_to_client, conn);

        // 5. Reset action
        conn.action = ActiveAction::None;
    }
}
