
use std::sync::Arc;
use server_proxy::config::RouteConfig;
use server_proxy::http::Method;
use server_proxy::router::Router;

fn create_route_config(path: &str, methods: Vec<Method>) -> Arc<RouteConfig> {
    Arc::new(RouteConfig {
        path: path.to_string(),
        methods: methods.iter().map(|m| m.to_string()).collect(),
        ..Default::default()
    })
}

#[test]
fn test_router_simple_match() {
    let mut router = Router::new();
    let route_config = create_route_config("/", vec![Method::GET]);
    router.add_route_config("localhost", "/", route_config);

    let result = router.resolve(&Method::GET, "localhost", "/");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().path, "/");
}

#[test]
fn test_router_no_match() {
    let mut router = Router::new();
    let route_config = create_route_config("/", vec![Method::GET]);
    router.add_route_config("localhost", "/", route_config);

    let result = router.resolve(&Method::GET, "localhost", "/unconfigured");
    // still resolves to /
    assert!(result.is_ok());
}

#[test]
fn test_router_longest_prefix_match() {
    let mut router = Router::new();
    let route_config_a = create_route_config("/a", vec![Method::GET]);
    let route_config_ab = create_route_config("/a/b", vec![Method::GET]);
    router.add_route_config("localhost", "/a", route_config_a);
    router.add_route_config("localhost", "/a/b", route_config_ab);

    let result = router.resolve(&Method::GET, "localhost", "/a/b/c");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().path, "/a/b");
}

#[test]
fn test_router_method_not_allowed() {
    let mut router = Router::new();
    let route_config = create_route_config("/", vec![Method::GET]);
    router.add_route_config("localhost", "/", route_config);

    let result = router.resolve(&Method::POST, "localhost", "/");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), server_proxy::router::RoutingError::MethodNotAllowed));
}

#[test]
fn test_router_host_not_found() {
    let mut router = Router::new();
    let route_config = create_route_config("/", vec![Method::GET]);
    router.add_route_config("localhost", "/", route_config);

    let result = router.resolve(&Method::GET, "otherhost", "/");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), server_proxy::router::RoutingError::NotFound));
}

#[test]
fn test_router_path_not_found() {
    let mut router = Router::new();
    let route_config = create_route_config("/a", vec![Method::GET]);
    router.add_route_config("localhost", "/a", route_config);

    let result = router.resolve(&Method::GET, "localhost", "/b");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), server_proxy::router::RoutingError::NotFound));
}
