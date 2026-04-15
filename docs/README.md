# scrapling-rs Documentation

<p align="center">
  <img src="assets/logo.svg" alt="scrapling-rs" width="400">
</p>

Adaptive web scraping, built in Rust. A high-performance port of [Python Scrapling](https://github.com/D4Vinci/Scrapling).

## Documentation Index

### Getting Started
- [Overview](overview.md) — What scrapling-rs is, how to install it, and your first 30-second example

### Parsing HTML
- [Querying Elements](parsing/selection.md) — CSS selectors, text search, regex, compound filters, find_similar
- [Core Types](parsing/main_classes.md) — Selector, Selectors, TextHandler, AttributesHandler, navigation
- [Adaptive Scraping](parsing/adaptive.md) — Smart element relocation when websites change

### HTTP Fetching
- [Fetcher Overview](fetching/overview.md) — Choosing between HTTP, browser, and stealth fetching
- [HTTP Requests](fetching/http.md) — Fetcher, FetcherSession, impersonation, proxy rotation
- [Dynamic Websites](fetching/dynamic.md) — Browser automation with DynamicSession
- [Stealth Mode](fetching/stealth.md) — Anti-detection with StealthySession, Cloudflare bypass
- [Proxy and Blocking](fetching/proxy-blocking.md) — ProxyRotator, blocked request handling, ad blocking

### Spider Framework
- [Getting Started](spiders/getting-started.md) — Your first spider in 20 lines
- [Architecture](spiders/architecture.md) — How the crawl loop works
- [Sessions](spiders/sessions.md) — Managing HTTP and browser sessions
- [Requests and Responses](spiders/requests-responses.md) — Building requests, processing responses
- [Advanced](spiders/advanced.md) — Concurrency, robots.txt, checkpointing, streaming, dev mode

### Tools
- [CLI](cli/overview.md) — Command-line scraping without writing code
- [MCP Server](mcp-server.md) — AI agent integration (9 tools for Claude, Cursor, etc.)

### Tutorials
- [Migrating from Python Scrapling](tutorials/migrating-from-python.md) — Side-by-side comparison
- [Migrating from BeautifulSoup](tutorials/migrating-from-beautifulsoup.md) — Common operations mapped

### Reference
- [Performance](benchmarks.md) — Benchmarking and architecture advantages
- [Examples](https://github.com/chrisabruce/scrapling-rs/tree/main/examples) — 13 runnable examples
- [API Docs](https://docs.rs/scrapling) — Full rustdoc reference

## Quick links

| I want to... | Start here |
|--------------|------------|
| Parse HTML I already have | [Querying Elements](parsing/selection.md) |
| Fetch a page that blocks scrapers | [Stealth Mode](fetching/stealth.md) |
| Build a concurrent crawler | [Spider Getting Started](spiders/getting-started.md) |
| Use from the terminal | [CLI](cli/overview.md) |
| Connect to an AI agent | [MCP Server](mcp-server.md) |
| Migrate from Python | [Migration Guide](tutorials/migrating-from-python.md) |
