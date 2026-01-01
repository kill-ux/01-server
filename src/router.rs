use std::{collections::HashMap, sync::Arc};

use crate::{config::RouteConfig, http::*};

pub type Handler = fn(&HttpRequest) -> HttpResponse;

pub struct Router {
    // server_name|Path -> RouteConfig
    routes: HashMap<String, Arc<RouteConfig>>,
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn not_found() -> HttpResponse {
        HttpResponse::new(404, "NOT FOUND").set_body(b"404 - Page Not Found".to_vec(), "text/plain")
    }

    pub fn method_not_allowed() -> HttpResponse {
        HttpResponse::new(405, "METHOD NOT ALLOWED")
            .set_body(b"405 - Method Not Allowed".to_vec(), "text/plain")
    }

    pub fn add_route_config(&mut self, host: &str, path: &str, r_cfg: Arc<RouteConfig>) {
        self.routes
            .entry(format!("{}|{}", host, path))
            .or_insert(r_cfg);
    }

    pub fn resolve(
        &self,
        method: &Method,
        host: &str,
        path: &str,
    ) -> Result<Arc<RouteConfig>, RoutingError> {
        // Try exact match first
        let key = format!("{}|{}", host, path);
        dbg!(self.routes.keys());
        if let Some(r_cfg) = self.routes.get(key.as_str()) {
            return if method.is_allowed(&r_cfg.methods) {
                Ok(Arc::clone(r_cfg))
            } else {
                Err(RoutingError::MethodNotAllowed)
            };
        }

        // Fallback to prefix matching
        let mut best_match: Option<(&String, &Arc<RouteConfig>)> = None;
        for (path_prefix, r_cfg) in &self.routes {
            if path_prefix.starts_with(&format!("{}|", host)) {
                let prefix_path = &path_prefix[host.len() + 1..]; // Skip "host|"
                if path.starts_with(prefix_path) {
                    if let Some((prev_path, _)) = best_match {
                        if prev_path.len() < prefix_path.len() {
                            best_match = Some((path_prefix, r_cfg));
                        }
                    } else {
                        best_match = Some((path_prefix, r_cfg));
                    }
                }
            }
        }

        let (_, r_cfg) = best_match.ok_or(RoutingError::NotFound)?;

        if method.is_allowed(&r_cfg.methods) {
            Ok(Arc::clone(r_cfg))
        } else {
            Err(RoutingError::MethodNotAllowed)
        }
    }
}

#[derive(Debug)]
pub enum RoutingError {
    NotFound,
    MethodNotAllowed,
}
