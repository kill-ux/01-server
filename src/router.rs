use std::{collections::HashMap, sync::Arc};

use crate::{config::RouteConfig, http::*};

pub type Handler = fn(&HttpRequest) -> HttpResponse;

pub struct Router {
    // Key: "port|host|path" -> RouteConfig
    pub routes: HashMap<String, Arc<RouteConfig>>,
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

    pub fn resolve(
        &self,
        port: u16,
        host: &str,
        path: &str,
        method: &Method,
    ) -> Result<Arc<RouteConfig>, RoutingError> {
        // Try specific host first, then fallback to catch-all "_"
        let hosts_to_try = [host, "_"];

        for h in hosts_to_try {
            let mut best_match: Option<(&String, &Arc<RouteConfig>)> = None;
            let prefix_start = format!("{}|{}|", port, h);

            for (key, r_cfg) in &self.routes {
                if key.starts_with(&prefix_start) {
                    let route_path = &key[prefix_start.len()..];
                    if path.starts_with(route_path) {
                        // Longest Prefix Match
                        if best_match.is_none() || route_path.len() > best_match.unwrap().0.len() {
                            best_match = Some((key, r_cfg));
                        }
                    }
                }
            }

            if let Some((_, r_cfg)) = best_match {
                if method.is_allowed(&r_cfg.methods) {
                    return Ok(Arc::clone(r_cfg));
                } else {
                    return Err(RoutingError::MethodNotAllowed);
                }
            }
        }
        Err(RoutingError::NotFound)
    }
}

#[derive(Debug)]
pub enum RoutingError {
    NotFound,
    MethodNotAllowed,
}
