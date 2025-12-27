use std::{collections::HashMap, sync::Arc};

use crate::{
    config::RouteConfig,
    http::*,
};

pub type Handler = fn(&HttpRequest) -> HttpResponse;

pub struct Router {
    routes: HashMap<Method, HashMap<String, HashMap<String, Arc<RouteConfig>>>>,
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

    pub fn not_found() -> HttpResponse {
        HttpResponse::new(404, "NOT FOUND").set_body(b"404 - Page Not Found".to_vec(), "text/plain")
    }

    pub fn add_route_config(
        &mut self,
        method: &Method,
        host: &str,
        path: &str,
        config: Arc<RouteConfig>,
    ) {
        self.routes
            .get_mut(method)
            .unwrap()
            .entry(host.to_string())
            .or_default()
            .insert(path.to_string(), config);
    }

    pub fn resolve(&self, method: &Method, host: &str, url: &str) -> Option<Arc<RouteConfig>> {
        let host_map = self.routes.get(method)?;
        let paths = host_map.get(host).or_else(|| host_map.get("default"))?;

        let mut best_match: Option<(&String, &Arc<RouteConfig>)> = None;
        for (path_prefix, r_cfg) in paths {
            if url.starts_with(path_prefix) {
                if let Some((prev_path,_ )) = best_match && prev_path.len() > path_prefix.len() {
                    continue;
                }
                best_match = Some((path_prefix, r_cfg));
            }
        }

        best_match.map(|(_,r_cfg)| Arc::clone(r_cfg))
    }
}
