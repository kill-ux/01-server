use crate::prelude::*;

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

    pub fn set_header(&mut self, key: &str, value: &str) -> &mut Self {
        self.headers.insert(key.to_lowercase(), value.to_string());
        self
    }

    pub fn set_body(&mut self, body: Vec<u8>, content_type: &str) -> &mut Self {
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
            let formatted_key = Self::to_pascal_case(key);
            res.extend_from_slice(format!("{}: {}\r\n", formatted_key, val).as_bytes());
        }
        res.extend_from_slice(b"\r\n");
        res.extend_from_slice(&self.body);
        res
    }

    pub fn status_text(code: u16) -> String {
        match code {
            HTTP_NOT_FOUND => "NOT FOUND".to_string(),
            _ => "Ok".to_string(),
        }
    }

    pub fn set_status_code(&mut self, code: u16) -> &mut Self {
        self.status_code = code;
        self.status_text = HttpResponse::status_text(code);
        self
    }

    pub fn to_bytes_headers_only(&self) -> Vec<u8> {
        let mut res = format!("HTTP/1.1 {} {}\r\n", self.status_code, self.status_text);

        for (k, v) in &self.headers {
            let formatted_key = Self::to_pascal_case(k);
            res.push_str(&format!("{}: {}\r\n", formatted_key, v));
        }

        res.push_str("\r\n");
        res.into_bytes()
    }

    fn to_pascal_case(s: &str) -> String {
        s.split('-')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<String>>()
            .join("-")
    }

    pub fn redirect(res: &mut HttpResponse, code: u16, target_url: &str) -> Self {
        let status_text = match code {
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            _ => "Found",
        };

        let mut res = HttpResponse::new(code, status_text);
        res.set_header("Location", target_url)
            .set_header("Content-Length", "0")
            .set_header("Connection", "close");

        res
    }
}

pub fn get_mime_type(extension: Option<&str>) -> &'static str {
    match extension {
        Some("html") | Some("htm") => "text/html",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

pub fn get_ext_from_content_type(content_type: &str) -> &str {
    match content_type {
        "application/json" => ".json",
        "application/pdf" => ".pdf",
        "application/xml" => ".xml",
        "application/zip" => ".zip",
        "audio/mpeg" => ".mp3",
        "image/gif" => ".gif",
        "image/jpeg" => ".jpg",
        "image/png" => ".png",
        "image/svg+xml" => ".svg",
        "image/webp" => ".webp",
        "text/css" => ".css",
        "text/html" => ".html",
        "text/javascript" => ".js",
        "text/plain" => ".txt",
        "video/mp4" => ".mp4",
        _ => ".bin",
    }
}

pub fn generate_autoindex(path: &Path, original_url: &str) -> HttpResponse {
    let mut html = format!("<html><body><h1>Index of {}</h1><ul>", original_url);
    if let Ok(entries) = path.read_dir() {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                html.push_str(&format!(
                    "<li><a href=\"{}/{}\">{}</a></li>",
                    original_url.trim_end_matches('/'),
                    name,
                    name
                ));
            }
        }
    }

    html.push_str("</ul></body></html>");
    let mut res = HttpResponse::new(200, "OK") ;
    res.set_body(html.into_bytes(), "text/html");
    res
}

pub fn handle_error(res: &mut HttpResponse,code: u16, s_cfg: Option<&Arc<ServerConfig>>) {
    let status_text = match code {
        HTTP_BAD_REQUEST => "Bad Request",
        HTTP_FORBIDDEN => "Forbidden",
        HTTP_NOT_FOUND => "Not Found",
        HTTP_METHOD_NOT_ALLOWED => "Method Not Allowed",
        HTTP_PAYLOAD_TOO_LARGE => "Payload Too Large",
        HTTP_URI_TOO_LONG => "URI Too Long",
        HTTP_NOT_IMPLEMENTED => "Not Implemented",
        GATEWAY_TIMEOUT => "GATEWAY TIMEOUT",
        _ => "Internal Server Error",
    };

    if let Some(cfg) = s_cfg {
        if let Some(path_str) = cfg.error_pages.get(&code) {
            let s_root = std::path::Path::new(&cfg.root);
            let err_path = s_root.join(path_str.trim_start_matches('/'));
            if let Ok(content) = fs::read(err_path) {
                res.set_status_code(code).set_body(content, "text/html");

                if code >= 400 && code != 404 && code != 405 {
                    res.headers
                        .insert("connection".to_string(), "close".to_string());
                } else {
                    res.headers
                        .insert("connection".to_string(), "keep-alive".to_string());
                }

                return;
            }
        }
    }

    res.set_status_code(code);

    let body = format!("{} {}", code, status_text).into_bytes();
    if code >= 400 && code != 404 && code != 405 {
        res.headers
            .insert("connection".to_string(), "close".to_string());
    } else {
        res.headers
            .insert("connection".to_string(), "keep-alive".to_string());
    }
    res.set_body(body, "text/plain");
}
