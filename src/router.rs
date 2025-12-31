use std::{collections::HashMap, sync::Arc};

use crate::{config::RouteConfig, http::*};

pub type Handler = fn(&HttpRequest) -> HttpResponse;

pub struct Router {
    // server_name -> Path -> RouteConfig
    routes: HashMap<String, HashMap<String, Arc<RouteConfig>>>,
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
            .entry(host.to_string())
            .or_default()
            .insert(path.to_string(), r_cfg);
    }

    pub fn resolve(
        &self,
        method: &Method,
        host: &str,
        url: &str,
    ) -> Result<Arc<RouteConfig>, RoutingError> {
        // let host_map = self.routes.get(method)?;
        let paths = self
            .routes
            .get(host)
            .ok_or(RoutingError::NotFound)?;

        let mut best_match: Option<(&String, &Arc<RouteConfig>)> = None;
        for (path_prefix, r_cfg) in paths {
            if url.starts_with(path_prefix) {
                if let Some((prev_path, _)) = best_match
                    && prev_path.len() > path_prefix.len()
                {
                    continue;
                }
                best_match = Some((path_prefix, r_cfg));
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
