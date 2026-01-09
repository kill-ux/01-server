use crate::prelude::*;

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
}

#[derive(Debug)]
pub enum RoutingError {
    NotFound,
    MethodNotAllowed,
}
