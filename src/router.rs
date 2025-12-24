use std::collections::HashMap;

use crate::http::*;

pub type Handler = fn(&HttpRequest) -> Vec<u8>;

pub struct Router {
    routes: HashMap<Method, HashMap<String, Handler>>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::from([
                (Method::GET, HashMap::new()),
                (Method::POST, HashMap::new()),
                (Method::DELETE, HashMap::new()),
            ]),
        }
    }

    pub fn add_route(&mut self, method: Method, path: &str, handler: Handler) {
        if let Some(method_request) = self.routes.get_mut(&method) {
            method_request.insert(path.to_string(), handler);
        }
    }

    pub fn route(&self, request: &HttpRequest) -> Vec<u8> {
        let handler = self
            .routes
            .get(&request.method)
            .and_then(|method_request| method_request.get(&request.url));

        match handler {
            Some(handler_func) => handler_func(request),
            None => self.not_found(),
        }
    }

    fn not_found(&self) -> Vec<u8> {
        b"HTTP/1.1 404 NOT FOUND\r\nContent-Length: 9\r\n\r\nNot Found".to_vec()
    }
}
