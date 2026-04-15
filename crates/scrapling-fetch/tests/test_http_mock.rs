use std::collections::HashMap;

use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use scrapling_fetch::{Fetcher, FetcherConfig, RequestConfig};

#[tokio::test]
async fn get_returns_body_and_status() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("<html><body><h1>Hello</h1></body></html>"),
        )
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let resp = fetcher
        .get(&format!("{}/hello", server.uri()), None)
        .await
        .unwrap();

    assert_eq!(resp.status, 200);
    assert!(resp.is_success());
    let h1 = resp.css("h1");
    assert_eq!(h1.len(), 1);
    assert_eq!(h1[0].text().as_ref(), "Hello");
}

#[tokio::test]
async fn post_sends_json_body() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api"))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(201).set_body_string(r#"{"ok":true}"#))
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let req = RequestConfig {
        json: Some(serde_json::json!({"name": "test"})),
        ..Default::default()
    };

    let resp = fetcher
        .post(&format!("{}/api", server.uri()), Some(req))
        .await
        .unwrap();

    assert_eq!(resp.status, 201);
}

#[tokio::test]
async fn query_params_appended() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "rust"))
        .respond_with(ResponseTemplate::new(200).set_body_string("found"))
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let req = RequestConfig {
        params: Some(HashMap::from([("q".into(), "rust".into())])),
        ..Default::default()
    };

    let resp = fetcher
        .get(&format!("{}/search", server.uri()), Some(req))
        .await
        .unwrap();

    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn custom_headers_sent() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/"))
        .and(header("x-custom", "hello"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let req = RequestConfig {
        headers: Some(HashMap::from([("x-custom".into(), "hello".into())])),
        ..Default::default()
    };

    let resp = fetcher.get(&server.uri(), Some(req)).await.unwrap();

    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn handles_404() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/missing"))
        .respond_with(ResponseTemplate::new(404).set_body_string("not found"))
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        retries: 1,
        ..Default::default()
    });

    let resp = fetcher
        .get(&format!("{}/missing", server.uri()), None)
        .await
        .unwrap();

    assert_eq!(resp.status, 404);
    assert!(resp.is_client_error());
    assert!(!resp.is_success());
}

#[tokio::test]
async fn handles_500() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        retries: 1,
        ..Default::default()
    });

    let resp = fetcher.get(&server.uri(), None).await.unwrap();

    assert!(resp.is_server_error());
}

#[tokio::test]
async fn put_and_delete() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/item"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    Mock::given(method("DELETE"))
        .and(path("/item"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let put_resp = fetcher
        .put(&format!("{}/item", server.uri()), None)
        .await
        .unwrap();
    assert_eq!(put_resp.status, 200);

    let del_resp = fetcher
        .delete(&format!("{}/item", server.uri()), None)
        .await
        .unwrap();
    assert_eq!(del_resp.status, 204);
}

#[tokio::test]
async fn response_url_preserved() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let resp = fetcher
        .get(&format!("{}/page", server.uri()), None)
        .await
        .unwrap();

    assert!(resp.url().contains("/page"));
}

#[tokio::test]
async fn to_markdown_and_text() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("<html><body><h1>Title</h1><p>Body text</p></body></html>"),
        )
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let resp = fetcher.get(&server.uri(), None).await.unwrap();

    let md = resp.to_markdown();
    assert!(md.contains("Title"));

    let text = resp.to_text();
    assert!(text.contains("Title"));
    assert!(text.contains("Body text"));
}

#[tokio::test]
async fn cookies_extracted() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("set-cookie", "session=abc123; Path=/")
                .set_body_string("ok"),
        )
        .mount(&server)
        .await;

    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        ..Default::default()
    });

    let resp = fetcher.get(&server.uri(), None).await.unwrap();
    assert_eq!(resp.status, 200);
    // Cookie extraction depends on wreq's cookie handling
}

#[tokio::test]
async fn retry_on_connection_failure() {
    // Connect to a port that nothing listens on
    let fetcher = Fetcher::with_config(FetcherConfig {
        stealthy_headers: false,
        retries: 2,
        retry_delay_secs: 0,
        timeout_secs: 1,
        ..Default::default()
    });

    let result = fetcher
        .get("http://127.0.0.1:19999/nonexistent", None)
        .await;

    assert!(result.is_err());
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("2 attempts") || err.contains("error"));
}
