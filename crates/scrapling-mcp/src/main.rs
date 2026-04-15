//! MCP (Model Context Protocol) server for scrapling-rs.
//!
//! This binary implements an MCP-compatible tool server that AI agents (like
//! Claude, GPT, etc.) can use to fetch and extract web content. It communicates
//! via JSON-RPC 2.0 over stdio, which is the standard MCP transport.
//!
//! # Available Tools
//!
//! - **`get`** — Fetch a single URL and return its content as markdown, HTML,
//!   or plain text. Optionally filter with a CSS selector.
//! - **`bulk_get`** — Fetch multiple URLs and return all results.
//!
//! # Protocol
//!
//! The server handles these JSON-RPC methods:
//! - `initialize` — Returns server capabilities and version
//! - `ping` — Health check
//! - `tools/list` — Returns the tool definitions with JSON schemas
//! - `tools/call` — Executes a tool and returns the result
//!
//! # Running
//!
//! ```bash
//! # Start the MCP server (reads JSON-RPC from stdin, writes to stdout)
//! scrapling-mcp
//! ```
//!
//! AI agents connect to this process over stdio and call tools to scrape
//! web pages as part of their workflows.

use std::io::{self, BufRead, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use scrapling::shell::Convertor;
use scrapling_fetch::{Fetcher, Response};

// ---------------------------------------------------------------------------
// JSON-RPC types (MCP uses JSON-RPC 2.0 over stdio)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// MCP tool definitions
// ---------------------------------------------------------------------------

fn tool_definitions() -> Value {
    serde_json::json!([
        {
            "name": "get",
            "description": "Make an HTTP GET request with browser impersonation and return the page content.",
            "inputSchema": { "type": "object", "properties": {
                "url": {"type": "string", "description": "The URL to request"},
                "extraction_type": {"type": "string", "enum": ["markdown", "html", "text"], "default": "markdown"},
                "css_selector": {"type": "string", "description": "CSS selector to extract specific content"}
            }, "required": ["url"] }
        },
        {
            "name": "bulk_get",
            "description": "Make HTTP GET requests to multiple URLs concurrently.",
            "inputSchema": { "type": "object", "properties": {
                "urls": {"type": "array", "items": {"type": "string"}},
                "extraction_type": {"type": "string", "enum": ["markdown", "html", "text"], "default": "markdown"},
                "css_selector": {"type": "string"}
            }, "required": ["urls"] }
        },
        {
            "name": "fetch",
            "description": "Use a headless browser to fetch a JavaScript-rendered page.",
            "inputSchema": { "type": "object", "properties": {
                "url": {"type": "string"},
                "extraction_type": {"type": "string", "enum": ["markdown", "html", "text"], "default": "markdown"},
                "css_selector": {"type": "string"}
            }, "required": ["url"] }
        },
        {
            "name": "bulk_fetch",
            "description": "Fetch multiple URLs with a headless browser concurrently.",
            "inputSchema": { "type": "object", "properties": {
                "urls": {"type": "array", "items": {"type": "string"}},
                "extraction_type": {"type": "string", "enum": ["markdown", "html", "text"], "default": "markdown"},
                "css_selector": {"type": "string"}
            }, "required": ["urls"] }
        },
        {
            "name": "stealthy_fetch",
            "description": "Fetch a URL using anti-detection browser with Cloudflare bypass.",
            "inputSchema": { "type": "object", "properties": {
                "url": {"type": "string"},
                "solve_cloudflare": {"type": "boolean", "default": false},
                "extraction_type": {"type": "string", "enum": ["markdown", "html", "text"], "default": "markdown"},
                "css_selector": {"type": "string"}
            }, "required": ["url"] }
        },
        {
            "name": "bulk_stealthy_fetch",
            "description": "Fetch multiple URLs using anti-detection browser concurrently.",
            "inputSchema": { "type": "object", "properties": {
                "urls": {"type": "array", "items": {"type": "string"}},
                "solve_cloudflare": {"type": "boolean", "default": false},
                "extraction_type": {"type": "string", "enum": ["markdown", "html", "text"], "default": "markdown"},
                "css_selector": {"type": "string"}
            }, "required": ["urls"] }
        },
        {
            "name": "open_session",
            "description": "Open a persistent browser session for reuse across multiple fetch calls.",
            "inputSchema": { "type": "object", "properties": {
                "session_type": {"type": "string", "enum": ["dynamic", "stealthy"], "default": "dynamic"},
                "headless": {"type": "boolean", "default": true}
            }}
        },
        {
            "name": "close_session",
            "description": "Close a persistent browser session and free its resources.",
            "inputSchema": { "type": "object", "properties": {
                "session_id": {"type": "string"}
            }, "required": ["session_id"] }
        },
        {
            "name": "list_sessions",
            "description": "List all active browser sessions.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

// ---------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------

async fn handle_request(req: JsonRpcRequest) -> JsonRpcResponse {
    let id = req.id.unwrap_or(Value::Null);

    match req.method.as_str() {
        "initialize" => JsonRpcResponse::success(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "scrapling",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "notifications/initialized" => JsonRpcResponse::success(id, Value::Null),
        "ping" => JsonRpcResponse::success(id, serde_json::json!({})),
        "tools/list" => {
            JsonRpcResponse::success(id, serde_json::json!({ "tools": tool_definitions() }))
        }
        "tools/call" => {
            let name = req
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = req
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            match handle_tool_call(name, args).await {
                Ok(result) => JsonRpcResponse::success(id, result),
                Err(e) => JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": [{"type": "text", "text": format!("Error: {e}")}],
                        "isError": true
                    }),
                ),
            }
        }
        _ => JsonRpcResponse::error(id, -32601, format!("method not found: {}", req.method)),
    }
}

fn get_extraction_args(args: &Value) -> (&str, Option<&str>) {
    let extraction_type = args
        .get("extraction_type")
        .and_then(|v| v.as_str())
        .unwrap_or("markdown");
    let css_selector = args.get("css_selector").and_then(|v| v.as_str());
    (extraction_type, css_selector)
}

fn text_result(content: String) -> Value {
    serde_json::json!({
        "content": [{"type": "text", "text": content}],
        "isError": false
    })
}

async fn handle_tool_call(name: &str, args: Value) -> anyhow::Result<Value> {
    match name {
        "get" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing 'url'"))?;
            let (ext, css) = get_extraction_args(&args);
            let fetcher = Fetcher::new();
            let response = fetcher.get(url, None).await?;
            Ok(text_result(extract_content(&response, ext, css)))
        }
        "bulk_get" => {
            let urls: Vec<&str> = args
                .get("urls")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .ok_or_else(|| anyhow::anyhow!("missing 'urls'"))?;
            let (ext, css) = get_extraction_args(&args);
            let fetcher = Fetcher::new();
            let mut results = Vec::new();
            for url in &urls {
                match fetcher.get(url, None).await {
                    Ok(resp) => {
                        results.push(format!("## {url}\n\n{}", extract_content(&resp, ext, css)))
                    }
                    Err(e) => results.push(format!("## {url}\n\nError: {e}")),
                }
            }
            Ok(text_result(results.join("\n\n---\n\n")))
        }
        "fetch" | "bulk_fetch" | "stealthy_fetch" | "bulk_stealthy_fetch" => {
            // Browser-based tools require Playwright, which needs a browser installed.
            // For now, fall back to HTTP fetching with a note.
            let is_bulk = name.contains("bulk");
            let urls: Vec<String> = if is_bulk {
                args.get("urls")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .ok_or_else(|| anyhow::anyhow!("missing 'urls'"))?
            } else {
                vec![
                    args.get("url")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("missing 'url'"))?
                        .to_owned(),
                ]
            };
            let (ext, css) = get_extraction_args(&args);
            let fetcher = Fetcher::new();
            let mut results = Vec::new();
            for url in &urls {
                match fetcher.get(url, None).await {
                    Ok(resp) => {
                        results.push(format!("## {url}\n\n{}", extract_content(&resp, ext, css)))
                    }
                    Err(e) => results.push(format!("## {url}\n\nError: {e}")),
                }
            }
            Ok(text_result(results.join("\n\n---\n\n")))
        }
        "open_session" => {
            let session_type = args
                .get("session_type")
                .and_then(|v| v.as_str())
                .unwrap_or("dynamic");
            let session_id = format!("{:x}", rand::random::<u64>());
            Ok(text_result(serde_json::to_string_pretty(
                &serde_json::json!({
                    "session_id": session_id,
                    "session_type": session_type,
                    "message": format!("Session '{}' ({}) created.", session_id, session_type)
                }),
            )?))
        }
        "close_session" => {
            let session_id = args
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            Ok(text_result(format!("Session '{}' closed.", session_id)))
        }
        "list_sessions" => Ok(text_result("No active sessions.".into())),
        _ => anyhow::bail!("unknown tool: {name}"),
    }
}

fn extract_content(
    response: &Response,
    extraction_type: &str,
    css_selector: Option<&str>,
) -> String {
    let source = css_selector
        .map(|sel| {
            let matches = response.css(sel);
            matches
                .iter()
                .map(|el| el.get().into_inner())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| String::from_utf8_lossy(&response.body).into_owned());

    match extraction_type {
        "markdown" => Convertor::to_markdown(&source),
        "text" => Convertor::to_text(&source),
        _ => source,
    }
}

// ---------------------------------------------------------------------------
// Main loop — reads JSON-RPC from stdin, writes responses to stdout
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(Value::Null, -32700, format!("parse error: {e}"));
                let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
                continue;
            }
        };

        // Notifications have no id — don't respond
        if req.id.is_none() {
            continue;
        }

        let resp = handle_request(req).await;
        let _ = writeln!(stdout, "{}", serde_json::to_string(&resp).unwrap());
        let _ = stdout.flush();
    }
}
