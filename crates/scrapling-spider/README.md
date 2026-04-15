# scrapling-spider

Concurrent web crawler framework for [scrapling-rs](https://github.com/chrisabruce/scrapling-rs).

## Features

- **Concurrent crawling** with configurable parallelism
- **Request deduplication** via SHA-1 fingerprinting
- **Robots.txt compliance** with crawl-delay support
- **Checkpoint/resume** for long-running crawls
- **Development mode** with response caching
- Built on scrapling-fetch for TLS impersonation and scrapling-browser for JS-rendered pages

## License

MIT
