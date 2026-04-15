//! Command-line interface for scrapling-rs.
//!
//! This binary provides the `scrapling` CLI tool for quick web scraping tasks
//! from the terminal. It wraps the `scrapling-fetch` HTTP client with a
//! user-friendly command structure powered by `clap`.
//!
//! # Commands
//!
//! - **`extract get/post/put/delete`** — Make HTTP requests and save the response
//!   to a file. Supports CSS selector extraction, custom headers, cookies, proxy,
//!   browser impersonation, and output as HTML, text, or JSON.
//! - **`info`** — Show which scrapling-rs components are compiled in.
//! - **`shell`** — Placeholder for an interactive scraping REPL.
//!
//! # Examples
//!
//! ```bash
//! # Fetch a page and save as HTML
//! scrapling extract get https://example.com output.html
//!
//! # Extract specific elements with a CSS selector
//! scrapling extract get https://example.com output.html -s "h1, p"
//!
//! # POST JSON data
//! scrapling extract post https://api.example.com response.json -j '{"key":"value"}'
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use scrapling_fetch::{Fetcher, FetcherConfig, FollowRedirects, Impersonate, RequestConfig};

#[derive(Parser)]
#[command(name = "scrapling", about = "Web scraping CLI powered by scrapling-rs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Extract web content via HTTP requests
    Extract {
        #[command(subcommand)]
        method: ExtractMethod,
    },
    /// Show compiled features and dependency info
    Info,
    /// Launch an interactive scraping shell
    Shell,
}

#[derive(Subcommand)]
enum ExtractMethod {
    /// Perform a GET request and save the result
    Get {
        /// Target URL
        url: String,
        /// Output file path (.html, .txt, or .json)
        output: PathBuf,
        #[command(flatten)]
        opts: HttpOptions,
    },
    /// Perform a POST request and save the result
    Post {
        /// Target URL
        url: String,
        /// Output file path (.html, .txt, or .json)
        output: PathBuf,
        #[command(flatten)]
        opts: HttpOptions,
        #[command(flatten)]
        data_opts: DataOptions,
    },
    /// Perform a PUT request and save the result
    Put {
        /// Target URL
        url: String,
        /// Output file path (.html, .txt, or .json)
        output: PathBuf,
        #[command(flatten)]
        opts: HttpOptions,
        #[command(flatten)]
        data_opts: DataOptions,
    },
    /// Perform a DELETE request and save the result
    Delete {
        /// Target URL
        url: String,
        /// Output file path (.html, .txt, or .json)
        output: PathBuf,
        #[command(flatten)]
        opts: HttpOptions,
    },
}

#[derive(clap::Args)]
struct HttpOptions {
    /// CSS selector to extract specific content
    #[arg(short = 's', long)]
    css_selector: Option<String>,

    /// HTTP headers in "Key: Value" format
    #[arg(short = 'H', long = "header", num_args = 1)]
    headers: Vec<String>,

    /// Query parameters in "key=value" format
    #[arg(short = 'p', long = "param", num_args = 1)]
    params: Vec<String>,

    /// Cookies in "name1=value1; name2=value2" format
    #[arg(long)]
    cookies: Option<String>,

    /// Proxy URL (e.g. http://user:pass@proxy:8080)
    #[arg(long)]
    proxy: Option<String>,

    /// Request timeout in seconds [default: 30]
    #[arg(long, default_value = "30")]
    timeout: u64,

    /// Browser to impersonate (comma-separated for random selection)
    #[arg(long)]
    impersonate: Option<String>,

    /// Enable stealth headers [default: true]
    #[arg(long, default_value = "true")]
    stealthy_headers: bool,

    /// Verify SSL certificates [default: true]
    #[arg(long, default_value = "true")]
    verify: bool,

    /// Follow redirects [default: true]
    #[arg(long, default_value = "true")]
    follow_redirects: bool,
}

#[derive(clap::Args)]
struct DataOptions {
    /// JSON data to send in the request body
    #[arg(short = 'j', long = "json")]
    json_data: Option<String>,

    /// Form data to send in the request body
    #[arg(short = 'd', long = "data")]
    form_data: Option<String>,
}

fn parse_kv_pairs(raw: &[String], delimiter: char) -> HashMap<String, String> {
    raw.iter()
        .filter_map(|s| s.split_once(delimiter))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

fn parse_headers(raw: &[String]) -> HashMap<String, String> {
    parse_kv_pairs(raw, ':')
}

fn parse_params(raw: &[String]) -> HashMap<String, String> {
    parse_kv_pairs(raw, '=')
}

fn parse_cookies(raw: &str) -> HashMap<String, String> {
    raw.split(';')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect()
}

fn build_impersonate(raw: &str) -> Impersonate {
    let parts: Vec<String> = raw.split(',').map(|s| s.trim().to_owned()).collect();
    match parts.len() {
        1 => Impersonate::Single(parts.into_iter().next().unwrap()),
        _ => Impersonate::Random(parts),
    }
}

fn build_fetcher_config(opts: &HttpOptions) -> FetcherConfig {
    let impersonate = opts
        .impersonate
        .as_deref()
        .map(build_impersonate)
        .unwrap_or_default();

    let follow_redirects = if opts.follow_redirects {
        FollowRedirects::All
    } else {
        FollowRedirects::None
    };

    FetcherConfig {
        impersonate,
        stealthy_headers: opts.stealthy_headers,
        proxy: opts
            .proxy
            .as_ref()
            .map(|p| scrapling_fetch::Proxy::Url(p.clone())),
        timeout_secs: opts.timeout,
        headers: parse_headers(&opts.headers),
        retries: 3,
        retry_delay_secs: 1,
        follow_redirects,
        max_redirects: 30,
        verify: opts.verify,
    }
}

fn build_request_config(opts: &HttpOptions, data_opts: Option<&DataOptions>) -> RequestConfig {
    let params = if opts.params.is_empty() {
        None
    } else {
        Some(parse_params(&opts.params))
    };

    let cookies = opts.cookies.as_deref().map(parse_cookies);

    let (json, data) = if let Some(d) = data_opts {
        let json = d.json_data.as_ref().map(|j| {
            serde_json::from_str(j).unwrap_or_else(|e| {
                eprintln!("Invalid JSON data: {e}");
                std::process::exit(1);
            })
        });
        let data = d.form_data.as_ref().map(|f| f.as_bytes().to_vec());
        (json, data)
    } else {
        (None, None)
    };

    RequestConfig {
        params,
        cookies,
        json,
        data,
        ..Default::default()
    }
}

async fn execute_request(
    method: &str,
    url: &str,
    output: &PathBuf,
    opts: &HttpOptions,
    data_opts: Option<&DataOptions>,
) -> anyhow::Result<()> {
    let config = build_fetcher_config(opts);
    let fetcher = Fetcher::with_config(config);
    let req = build_request_config(opts, data_opts);

    let response = match method {
        "GET" => fetcher.get(url, Some(req)).await,
        "POST" => fetcher.post(url, Some(req)).await,
        "PUT" => fetcher.put(url, Some(req)).await,
        "DELETE" => fetcher.delete(url, Some(req)).await,
        _ => unreachable!(),
    }
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    eprintln!("{}", response);

    let content = if let Some(ref selector) = opts.css_selector {
        let matches = response.css(selector);
        if matches.is_empty() {
            eprintln!("No elements matched selector: {selector}");
            String::new()
        } else {
            matches
                .iter()
                .map(|el| el.get().into_inner())
                .collect::<Vec<_>>()
                .join("\n")
        }
    } else {
        let ext = output
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("html");
        match ext {
            "txt" => response.text().into_inner(),
            "json" => {
                let texts = response.css("body::text");
                let all: Vec<String> = texts.iter().map(|t| t.text().into_inner()).collect();
                serde_json::to_string_pretty(&all).unwrap_or_default()
            }
            _ => String::from_utf8_lossy(&response.body).to_string(),
        }
    };

    output
        .parent()
        .filter(|p| !p.exists())
        .map(std::fs::create_dir_all)
        .transpose()?;
    std::fs::write(output, content)?;
    eprintln!("Saved to {}", output.display());

    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Extract { method } => match method {
            ExtractMethod::Get { url, output, opts } => {
                execute_request("GET", &url, &output, &opts, None).await
            }
            ExtractMethod::Post {
                url,
                output,
                opts,
                data_opts,
            } => execute_request("POST", &url, &output, &opts, Some(&data_opts)).await,
            ExtractMethod::Put {
                url,
                output,
                opts,
                data_opts,
            } => execute_request("PUT", &url, &output, &opts, Some(&data_opts)).await,
            ExtractMethod::Delete { url, output, opts } => {
                execute_request("DELETE", &url, &output, &opts, None).await
            }
        },
        Commands::Info => {
            println!("scrapling-rs v{}", env!("CARGO_PKG_VERSION"));
            println!("  core:    scrapling (HTML parsing, CSS selectors, adaptive relocation)");
            println!(
                "  fetch:   scrapling-fetch (HTTP with TLS impersonation via curl-impersonate)"
            );
            println!("  browser: scrapling-browser (Playwright automation with anti-detection)");
            println!("  spider:  scrapling-spider (concurrent crawler framework)");
            println!("  storage: SQLite backend (feature: storage, enabled by default)");
            Ok(())
        }
        Commands::Shell => {
            eprintln!("Interactive shell not yet implemented.");
            eprintln!("Use the Python version: scrapling shell");
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
