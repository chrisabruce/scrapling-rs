use scrapling_fetch::{Proxy, ProxyRotator, cyclic_rotation, is_proxy_error};

// ---------------------------------------------------------------------------
// Cyclic Rotation Strategy
// ---------------------------------------------------------------------------

#[test]
fn cyclic_rotation_cycles_through_proxies() {
    let proxies = vec![
        Proxy::Url("http://p1:8080".into()),
        Proxy::Url("http://p2:8080".into()),
        Proxy::Url("http://p3:8080".into()),
    ];

    let next = cyclic_rotation(&proxies, 0);
    assert_eq!(next, 1);

    let next = cyclic_rotation(&proxies, 1);
    assert_eq!(next, 2);

    let next = cyclic_rotation(&proxies, 2);
    assert_eq!(next, 3); // wraps on next get_proxy call via modulo
}

// ---------------------------------------------------------------------------
// ProxyRotator Creation
// ---------------------------------------------------------------------------

#[test]
fn create_with_string_proxies() {
    let proxies = vec![
        Proxy::Url("http://p1:8080".into()),
        Proxy::Url("http://p2:8080".into()),
    ];
    let rotator = ProxyRotator::new(proxies).unwrap();
    assert_eq!(rotator.len(), 2);
}

#[test]
fn create_with_dict_proxies() {
    let proxies = vec![
        Proxy::Config {
            server: "http://p1:8080".into(),
            username: Some("user1".into()),
            password: Some("pass1".into()),
        },
        Proxy::Config {
            server: "http://p2:8080".into(),
            username: None,
            password: None,
        },
    ];
    let rotator = ProxyRotator::new(proxies).unwrap();
    assert_eq!(rotator.len(), 2);
}

#[test]
fn create_with_mixed_proxies() {
    let proxies = vec![
        Proxy::Url("http://p1:8080".into()),
        Proxy::Config {
            server: "http://p2:8080".into(),
            username: Some("user".into()),
            password: None,
        },
    ];
    let rotator = ProxyRotator::new(proxies).unwrap();
    assert_eq!(rotator.len(), 2);
}

#[test]
fn empty_proxies_raises_error() {
    let result = ProxyRotator::new(vec![]);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// ProxyRotator Rotation Behavior
// ---------------------------------------------------------------------------

#[test]
fn get_proxy_cyclic_rotation() {
    let proxies = vec![
        Proxy::Url("http://p1:8080".into()),
        Proxy::Url("http://p2:8080".into()),
        Proxy::Url("http://p3:8080".into()),
    ];
    let rotator = ProxyRotator::new(proxies).unwrap();

    assert_eq!(rotator.get_proxy().server(), "http://p1:8080");
    assert_eq!(rotator.get_proxy().server(), "http://p2:8080");
    assert_eq!(rotator.get_proxy().server(), "http://p3:8080");

    // Wraps around
    assert_eq!(rotator.get_proxy().server(), "http://p1:8080");
    assert_eq!(rotator.get_proxy().server(), "http://p2:8080");
    assert_eq!(rotator.get_proxy().server(), "http://p3:8080");
}

#[test]
fn get_proxy_single_proxy() {
    let rotator = ProxyRotator::new(vec![Proxy::Url("http://only:8080".into())]).unwrap();
    for _ in 0..5 {
        assert_eq!(rotator.get_proxy().server(), "http://only:8080");
    }
}

#[test]
fn get_proxy_with_config_proxies() {
    let proxies = vec![
        Proxy::Config {
            server: "http://p1:8080".into(),
            username: None,
            password: None,
        },
        Proxy::Config {
            server: "http://p2:8080".into(),
            username: None,
            password: None,
        },
    ];
    let rotator = ProxyRotator::new(proxies).unwrap();

    assert_eq!(rotator.get_proxy().server(), "http://p1:8080");
    assert_eq!(rotator.get_proxy().server(), "http://p2:8080");
    assert_eq!(rotator.get_proxy().server(), "http://p1:8080");
}

// ---------------------------------------------------------------------------
// ProxyRotator Properties
// ---------------------------------------------------------------------------

#[test]
fn proxies_returns_slice() {
    let proxies = vec![
        Proxy::Url("http://p1:8080".into()),
        Proxy::Url("http://p2:8080".into()),
    ];
    let rotator = ProxyRotator::new(proxies).unwrap();
    assert_eq!(rotator.proxies().len(), 2);
}

#[test]
fn len_returns_proxy_count() {
    assert_eq!(
        ProxyRotator::new(vec![Proxy::Url("http://p1:8080".into())])
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ProxyRotator::new(vec![
            Proxy::Url("http://p1:8080".into()),
            Proxy::Url("http://p2:8080".into()),
        ])
        .unwrap()
        .len(),
        2
    );
}

#[test]
fn debug_repr() {
    let rotator = ProxyRotator::new(vec![
        Proxy::Url("http://p1:8080".into()),
        Proxy::Url("http://p2:8080".into()),
        Proxy::Url("http://p3:8080".into()),
    ])
    .unwrap();
    let repr = format!("{rotator:?}");
    assert!(repr.contains("3"));
}

// ---------------------------------------------------------------------------
// Thread Safety
// ---------------------------------------------------------------------------

#[test]
fn concurrent_get_proxy() {
    use std::sync::Arc;
    use std::thread;

    let proxies: Vec<Proxy> = (0..10)
        .map(|i| Proxy::Url(format!("http://p{i}:8080")))
        .collect();
    let proxy_servers: Vec<String> = proxies.iter().map(|p| p.server().to_owned()).collect();
    let rotator = Arc::new(ProxyRotator::new(proxies).unwrap());
    let mut handles = vec![];

    for _ in 0..10 {
        let rot = Arc::clone(&rotator);
        let servers = proxy_servers.clone();
        handles.push(thread::spawn(move || {
            let mut results = vec![];
            for _ in 0..100 {
                let p = rot.get_proxy();
                assert!(servers.contains(&p.server().to_owned()));
                results.push(p);
            }
            results
        }));
    }

    let mut all_results = vec![];
    for h in handles {
        all_results.extend(h.join().unwrap());
    }
    assert_eq!(all_results.len(), 1000);
}

// ---------------------------------------------------------------------------
// is_proxy_error
// ---------------------------------------------------------------------------

#[test]
fn proxy_errors_detected() {
    let proxy_msgs = [
        "net::err_proxy_connection_failed",
        "NET::ERR_PROXY_AUTH_FAILED",
        "net::err_tunnel_connection_failed",
        "Connection refused by proxy",
        "Connection reset by peer",
        "Connection timed out while connecting to proxy",
        "Failed to connect to proxy server",
        "Could not resolve proxy host",
    ];
    for msg in &proxy_msgs {
        let err: Box<dyn std::error::Error> = msg.to_string().into();
        assert!(
            is_proxy_error(err.as_ref()),
            "expected proxy error for: {msg}"
        );
    }
}

#[test]
fn non_proxy_errors_not_detected() {
    let non_proxy_msgs = [
        "Page not found",
        "404 Not Found",
        "Internal server error",
        "DNS resolution failed",
        "SSL certificate error",
        "Timeout waiting for response",
        "Invalid JSON response",
    ];
    for msg in &non_proxy_msgs {
        let err: Box<dyn std::error::Error> = msg.to_string().into();
        assert!(
            !is_proxy_error(err.as_ref()),
            "expected non-proxy error for: {msg}"
        );
    }
}

#[test]
fn case_insensitive_detection() {
    let cases = ["NET::ERR_PROXY", "Net::Err_Proxy", "CONNECTION REFUSED"];
    for msg in &cases {
        let err: Box<dyn std::error::Error> = msg.to_string().into();
        assert!(is_proxy_error(err.as_ref()), "failed for: {msg}");
    }
}

#[test]
fn empty_error_message() {
    let err: Box<dyn std::error::Error> = String::new().into();
    assert!(!is_proxy_error(err.as_ref()));
}
