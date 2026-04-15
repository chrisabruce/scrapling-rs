use scrapling_fetch::status_text;

#[test]
fn common_status_codes() {
    assert_eq!(status_text(200), "OK");
    assert_eq!(status_text(201), "Created");
    assert_eq!(status_text(204), "No Content");
    assert_eq!(status_text(301), "Moved Permanently");
    assert_eq!(status_text(302), "Found");
    assert_eq!(status_text(304), "Not Modified");
    assert_eq!(status_text(400), "Bad Request");
    assert_eq!(status_text(401), "Unauthorized");
    assert_eq!(status_text(403), "Forbidden");
    assert_eq!(status_text(404), "Not Found");
    assert_eq!(status_text(429), "Too Many Requests");
    assert_eq!(status_text(500), "Internal Server Error");
    assert_eq!(status_text(502), "Bad Gateway");
    assert_eq!(status_text(503), "Service Unavailable");
}

#[test]
fn informational_codes() {
    assert_eq!(status_text(100), "Continue");
    assert_eq!(status_text(101), "Switching Protocols");
    assert_eq!(status_text(102), "Processing");
    assert_eq!(status_text(103), "Early Hints");
}

#[test]
fn teapot() {
    assert_eq!(status_text(418), "I'm a teapot");
}

#[test]
fn unknown_status_code() {
    assert_eq!(status_text(999), "Unknown Status Code");
    assert_eq!(status_text(0), "Unknown Status Code");
}

// ---------------------------------------------------------------------------
// Fingerprint / Header Generation
// ---------------------------------------------------------------------------

use scrapling_fetch::fingerprint::{OsName, default_user_agent, detect_os, generate_headers};

#[test]
fn default_user_agent_contains_chrome() {
    let ua = default_user_agent();
    assert!(ua.contains("Chrome"), "expected Chrome in UA: {ua}");
    assert!(ua.contains("Mozilla/5.0"), "expected Mozilla prefix: {ua}");
}

#[test]
fn generate_headers_browser_mode_returns_chrome() {
    let headers = generate_headers(true);
    let ua = headers.get("User-Agent").expect("missing User-Agent");
    assert!(
        ua.contains("Chrome"),
        "browser_mode should use Chrome: {ua}"
    );
    assert!(headers.contains_key("Accept"));
    assert!(headers.contains_key("Accept-Language"));
}

#[test]
fn generate_headers_non_browser_mode_has_standard_headers() {
    let headers = generate_headers(false);
    assert!(headers.contains_key("User-Agent"));
    assert!(headers.contains_key("Accept"));
    assert!(headers.contains_key("Accept-Language"));
    assert!(headers.contains_key("Accept-Encoding"));
    assert!(headers.contains_key("Upgrade-Insecure-Requests"));
}

#[test]
fn detect_os_returns_valid_value() {
    let os = detect_os();
    assert!(matches!(
        os,
        OsName::Linux | OsName::MacOs | OsName::Windows
    ));
}

#[test]
fn generate_headers_includes_sec_headers_for_chrome() {
    // Run multiple times — browser_mode=false randomly picks a browser
    // but browser_mode=true always picks Chrome
    let headers = generate_headers(true);
    assert!(headers.contains_key("Sec-Ch-Ua"));
    assert!(headers.contains_key("Sec-Ch-Ua-Mobile"));
    assert!(headers.contains_key("Sec-Ch-Ua-Platform"));
    assert!(headers.contains_key("Sec-Fetch-Site"));
    assert!(headers.contains_key("Sec-Fetch-Mode"));
    assert!(headers.contains_key("Sec-Fetch-Dest"));
}

// ---------------------------------------------------------------------------
// Config Tests (equivalent to test_base.py)
// ---------------------------------------------------------------------------

use scrapling_fetch::{FetcherConfig, FollowRedirects, Impersonate};

#[test]
fn default_fetcher_config() {
    let config = FetcherConfig::default();
    assert!(config.stealthy_headers);
    assert_eq!(config.timeout_secs, 30);
    assert_eq!(config.retries, 3);
    assert_eq!(config.retry_delay_secs, 1);
    assert!(matches!(config.follow_redirects, FollowRedirects::Safe));
    assert_eq!(config.max_redirects, 30);
    assert!(config.verify);
    assert!(config.proxy.is_none());
    assert!(config.headers.is_empty());
}

#[test]
fn impersonate_single_selects_correctly() {
    let imp = Impersonate::Single("firefox".into());
    assert_eq!(imp.select(), Some("firefox"));
}

#[test]
fn impersonate_none_selects_none() {
    let imp = Impersonate::None;
    assert_eq!(imp.select(), None);
}

#[test]
fn impersonate_random_empty_list_selects_none() {
    let imp = Impersonate::Random(vec![]);
    assert_eq!(imp.select(), None);
}

#[test]
fn impersonate_random_selects_from_list() {
    let browsers = vec!["chrome".into(), "firefox".into(), "edge".into()];
    let imp = Impersonate::Random(browsers.clone());
    for _ in 0..20 {
        let selected = imp.select().unwrap();
        assert!(
            browsers.contains(&selected.to_owned()),
            "unexpected browser: {selected}"
        );
    }
}

#[test]
fn fetcher_config_builder() {
    let (config, rotator) = scrapling_fetch::FetcherConfigBuilder::new()
        .stealthy_headers(false)
        .timeout_secs(60)
        .retries(5)
        .verify(false)
        .follow_redirects(FollowRedirects::None)
        .build()
        .unwrap();

    assert!(!config.stealthy_headers);
    assert_eq!(config.timeout_secs, 60);
    assert_eq!(config.retries, 5);
    assert!(!config.verify);
    assert!(matches!(config.follow_redirects, FollowRedirects::None));
    assert!(rotator.is_none());
}

#[test]
fn fetcher_config_builder_rejects_proxy_and_rotator() {
    use scrapling_fetch::Proxy;

    let result = scrapling_fetch::FetcherConfigBuilder::new()
        .proxy(Proxy::Url("http://proxy:8080".into()))
        .proxy_rotator(ProxyRotator::new(vec![Proxy::Url("http://p1:8080".into())]).unwrap())
        .build();

    assert!(result.is_err());
}

use scrapling_fetch::ProxyRotator;

// ---------------------------------------------------------------------------
// Response Tests
// ---------------------------------------------------------------------------

use bytes::Bytes;
use scrapling_fetch::Response;
use std::collections::HashMap;

#[test]
fn response_status_helpers() {
    let resp = Response::new(
        "https://example.com",
        Bytes::from_static(b"<html><body>OK</body></html>"),
        200,
        None,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        "utf-8".into(),
        "GET".into(),
        vec![],
        HashMap::new(),
    );
    assert!(resp.is_success());
    assert!(!resp.is_redirect());
    assert!(!resp.is_client_error());
    assert!(!resp.is_server_error());
    assert_eq!(resp.reason, "OK");
    assert_eq!(resp.method, "GET");
}

#[test]
fn response_delegates_css_to_selector() {
    let html = b"<html><body><p class=\"msg\">Hello</p></body></html>";
    let resp = Response::new(
        "https://example.com",
        Bytes::from_static(html),
        200,
        None,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        "utf-8".into(),
        "GET".into(),
        vec![],
        HashMap::new(),
    );
    let msgs = resp.css("p.msg");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs.first().unwrap().text().as_ref(), "Hello");
}

#[test]
fn response_display() {
    let resp = Response::new(
        "https://example.com/page",
        Bytes::from_static(b"<html></html>"),
        404,
        None,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        "utf-8".into(),
        "GET".into(),
        vec![],
        HashMap::new(),
    );
    let display = format!("{resp}");
    assert!(display.contains("404"));
    assert!(display.contains("example.com"));
}

#[test]
fn response_urljoin() {
    let resp = Response::new(
        "https://example.com/path/page",
        Bytes::from_static(b"<html></html>"),
        200,
        None,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        "utf-8".into(),
        "GET".into(),
        vec![],
        HashMap::new(),
    );
    assert_eq!(resp.urljoin("/other"), "https://example.com/other");
    assert_eq!(resp.urljoin("sibling"), "https://example.com/path/sibling");
}
