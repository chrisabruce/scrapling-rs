//! HTTP fetching — make requests with browser impersonation.

use scrapling_fetch::{Fetcher, FetcherConfig, Impersonate, RequestConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simple GET request with default Chrome impersonation
    let fetcher = Fetcher::new();

    println!("Fetching httpbin.org...");
    let resp = fetcher.get("https://httpbin.org/get", None).await?;

    println!("Status: {}", resp.status);
    println!("URL: {}", resp.url());
    println!("Body length: {} bytes", resp.body.len());

    // Custom configuration
    let custom_fetcher = Fetcher::with_config(FetcherConfig {
        impersonate: Impersonate::Single("firefox".into()),
        stealthy_headers: true,
        timeout_secs: 10,
        retries: 2,
        ..Default::default()
    });

    // POST with JSON body
    let req = RequestConfig {
        json: Some(serde_json::json!({"key": "value"})),
        headers: Some(HashMap::from([("X-Custom".into(), "header-value".into())])),
        ..Default::default()
    };

    let resp = custom_fetcher
        .post("https://httpbin.org/post", Some(req))
        .await?;

    println!("\nPOST status: {}", resp.status);

    // Parse response as JSON
    let body: serde_json::Value = resp.selector().text().json()?;
    println!("Server saw headers: {}", body["headers"]);

    Ok(())
}
