.PHONY: build check test fmt clippy clean publish publish-dry-run

build:
	cargo build --workspace --exclude scrapling-python

check:
	cargo check --workspace --exclude scrapling-python

test:
	cargo test --workspace --exclude scrapling-python

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --exclude scrapling-python -- -D warnings

clean:
	cargo clean

publish-dry-run:
	cargo publish -p scrapling --dry-run
	cargo publish -p scrapling-fetch --dry-run
	cargo publish -p scrapling-browser --dry-run
	cargo publish -p scrapling-spider --dry-run
	cargo publish -p scrapling-cli --dry-run
	cargo publish -p scrapling-mcp --dry-run

publish:
	cargo publish -p scrapling
	sleep 30
	cargo publish -p scrapling-fetch
	sleep 30
	cargo publish -p scrapling-browser
	sleep 30
	cargo publish -p scrapling-spider
	sleep 30
	cargo publish -p scrapling-cli
	sleep 30
	cargo publish -p scrapling-mcp
