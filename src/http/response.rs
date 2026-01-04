use std::collections::HashMap;

#[derive(Debug)]
pub struct HttpResponse {
    pub version: String,
    pub status_code: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn new(status_code: u16, status_text: &str) -> Self {
        Self {
            version: "HTTP/1.1".to_string(),
            status_code,
            status_text: status_text.to_string(),
            headers: HashMap::from([("Content-Length".to_string(), "0".to_string())]),
            body: Vec::new(),
        }
    }

    pub fn set_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    pub fn set_body(mut self, body: Vec<u8>, content_type: &str) -> Self {
        self.headers
            .insert("Content-Length".to_string(), body.len().to_string());
        self.headers
            .insert("Content-Type".to_string(), content_type.to_string());
        self.body = body;
        self
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res = format!(
            "{} {} {}\r\n",
            self.version, self.status_code, self.status_text
        )
        .into_bytes();

        for (key, val) in &self.headers {
            res.extend_from_slice(format!("{}: {}\r\n", key, val).as_bytes());
        }
        res.extend_from_slice(b"\r\n");
        res.extend_from_slice(&self.body);
        res
    }

    pub fn redirect(code: u16, target_url: &str) -> Self {
        let status_text = match code {
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            _ => "Found",
        };

        HttpResponse::new(code, status_text)
            .set_header("Location", target_url)
            .set_header("Content-Length", "0")
            .set_header("Connection", "close")
    }
}
