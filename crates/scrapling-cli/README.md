# scrapling-cli

Command-line interface for [scrapling-rs](https://github.com/chrisabruce/scrapling-rs).

## Install

```bash
cargo install scrapling-cli
```

## Usage

```bash
# Fetch a page and extract with CSS selectors
scrapling fetch https://example.com --css "h1::text"

# Convert to markdown
scrapling fetch https://example.com --markdown
```

## License

MIT
