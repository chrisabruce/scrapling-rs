# scrapling-fetch

HTTP fetcher with TLS fingerprint impersonation for [scrapling-rs](https://github.com/chrisabruce/scrapling-rs).

## Features

- **135+ browser emulation profiles** (Chrome, Firefox, Safari, Edge, Opera, OkHttp) via wreq
- **TLS fingerprint impersonation** (JA3/JA4/HTTP2 APERT) so anti-bot systems see a real browser
- **Proxy rotation** with pluggable strategies
- **Automatic retry** with configurable backoff
- **Stealth headers** with Google referer injection
- Responses integrate with scrapling's CSS selector engine

## Quick start

```rust
use scrapling_fetch::{Fetcher, FetcherConfig, Impersonate};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fetcher = Fetcher::with_config(FetcherConfig {
        impersonate: Impersonate::Single("chrome".into()),
        stealthy_headers: true,
        ..Default::default()
    });

    let response = fetcher.get("https://example.com", None).await?;
    println!("Status: {}", response.status);

    // Response has full CSS selector support
    let title = response.css("title::text");
    println!("Title: {}", title.first().unwrap().text());

    // Convert to markdown
    println!("{}", response.to_markdown());

    Ok(())
}
```

## License

MIT
