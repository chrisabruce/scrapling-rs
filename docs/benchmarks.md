# Performance

scrapling-rs is designed for throughput. Here's how to benchmark it and what to expect.

## Running benchmarks

The project includes a criterion benchmark suite covering the core operations:

```bash
cargo bench -p scrapling
```

This measures:
- **Small HTML parsing** — parsing a minimal document
- **Large HTML parsing** — 5,000 element documents
- **CSS selection** — querying 1,000 elements
- **Text extraction** — recursive text from 500 paragraphs
- **Adaptive relocation** — similarity scoring across a changed DOM

## Architecture advantages

### No GIL
Python's Global Interpreter Lock means only one thread can execute Python code at a time, even on multi-core machines. scrapling-rs has no such limitation. The spider framework uses tokio's async runtime to process many requests concurrently without thread contention on the scraping logic.

### Zero-copy DOM
The parsed HTML document is stored in an arena-allocated tree (`ego_tree` via `scraper`). Multiple `Selector` references into the same document share the tree via reference counting, avoiding copies when navigating or filtering elements.

### Lazy response parsing
The `Response` struct stores the raw body bytes and only parses HTML into a `Selector` when you first call `.css()`, `.text()`, or `.selector()`. If you're checking status codes or headers without touching the body, no parsing happens at all.

### Compiled selectors
CSS selectors are compiled once and cached. Repeated queries with the same selector string reuse the compiled form through an LRU cache.

## Compared to Python

The Python version of Scrapling benchmarks at roughly 2ms for text extraction on 5,000 elements. The Rust version targets sub-millisecond for the same workload. The gap widens with concurrent crawling, where Python's async model (limited by the GIL for CPU work) can't match tokio's true parallelism.

The HTTP layer is different too. Python uses `curl_cffi` (libcurl FFI). Rust uses `wreq` (native async HTTP with BoringSSL). Both support browser fingerprint impersonation, but the Rust version avoids the overhead of crossing FFI boundaries on every request.
