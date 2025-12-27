use std::sync::Arc;

use server_proxy::{config::RouteConfig, http::Method, router::Router};

#[test]
fn test_longest_prefix_match() {
    let mut router = Router::new();
    let root_cfg = Arc::new(RouteConfig { root: "/root".into(), ..Default::default() });
    let api_cfg = Arc::new(RouteConfig { root: "/api".into(), ..Default::default() });

    router.add_route_config(&Method::GET, "127.0.0.1", "/", root_cfg);
    router.add_route_config(&Method::GET, "127.0.0.1", "/api", api_cfg);

    // Test exact match
    let res = router.resolve(&Method::GET, "127.0.0.1", "/api/users");
    assert_eq!(res.unwrap().root, "/api");

    // Test fallback to root
    let res = router.resolve(&Method::GET, "127.0.0.1", "/index.html");
    assert_eq!(res.unwrap().root, "/root");
}