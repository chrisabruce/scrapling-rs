//! Shell and conversion utilities.
//!
//! Provides:
//! - [`CurlParser`] — parse DevTools "Copy as cURL" commands
//! - [`parse_cookie_string`] / [`parse_headers`] — HTTP header/cookie parsing
//! - [`Convertor`] — HTML → Markdown / text conversion

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Cookie parsing
// ---------------------------------------------------------------------------

/// Parse a cookie string like `"name1=value1; name2=value2"` into key/value pairs.
pub fn parse_cookie_string(cookie_string: &str) -> Vec<(String, String)> {
    cookie_string
        .split(';')
        .filter_map(|pair| {
            let pair = pair.trim();
            if pair.is_empty() {
                return None;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Header parsing
// ---------------------------------------------------------------------------

/// Parse raw header lines (`"Key: Value"`) into a header map and optional cookie map.
///
/// If `parse_cookies` is `true`, any `Cookie` header is extracted separately
/// and its value is parsed via [`parse_cookie_string`].
pub fn parse_headers(
    header_lines: &[&str],
    parse_cookies: bool,
) -> (HashMap<String, String>, HashMap<String, String>) {
    let parsed: Vec<_> = header_lines
        .iter()
        .filter_map(|line| {
            let line = line.trim().trim_end_matches(';');
            line.split_once(':')
                .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        })
        .collect();

    let (cookie_lines, header_lines): (Vec<_>, Vec<_>) = parsed
        .into_iter()
        .partition(|(k, _)| parse_cookies && k.eq_ignore_ascii_case("cookie"));

    let headers = header_lines.into_iter().collect();
    let cookies = cookie_lines
        .into_iter()
        .flat_map(|(_, v)| parse_cookie_string(&v))
        .collect();

    (headers, cookies)
}

// ---------------------------------------------------------------------------
// Curl parser
// ---------------------------------------------------------------------------

/// A parsed curl command.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct CurlRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
    pub data: Option<String>,
    pub json_data: Option<serde_json::Value>,
    pub proxy: Option<String>,
    pub follow_redirects: bool,
}

/// Parse a "Copy as cURL" command from browser DevTools.
///
/// Handles `-X METHOD`, `-H "Header: Value"`, `-b "cookies"`, `-d "data"`,
/// `--data-raw`, `--data-binary`, `--json`, `-x proxy`, `-L`, and the URL.
pub fn parse_curl(curl_command: &str) -> std::result::Result<CurlRequest, crate::Error> {
    let tokens = shell_split(curl_command).map_err(crate::Error::Other)?;
    let mut req = CurlRequest {
        method: "GET".to_owned(),
        follow_redirects: false,
        ..Default::default()
    };

    let mut iter = tokens.into_iter();

    // Skip "curl" if present
    if let Some(first) = iter.next() {
        if first != "curl" && (first.starts_with("http://") || first.starts_with("https://")) {
            req.url = first;
        }
    }

    let mut explicit_method = false;

    while let Some(token) = iter.next() {
        match token.as_str() {
            "-X" | "--request" => {
                req.method = iter
                    .next()
                    .ok_or_else(|| crate::Error::Other("missing method after -X".into()))?
                    .to_uppercase();
                explicit_method = true;
            }
            "-H" | "--header" => {
                let header = iter
                    .next()
                    .ok_or_else(|| crate::Error::Other("missing header value after -H".into()))?;
                if let Some((key, value)) = header.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();
                    if key.eq_ignore_ascii_case("cookie") {
                        for (ck, cv) in parse_cookie_string(value) {
                            req.cookies.insert(ck, cv);
                        }
                    } else {
                        req.headers.insert(key.to_owned(), value.to_owned());
                    }
                }
            }
            "-b" | "--cookie" => {
                let cookie_str = iter
                    .next()
                    .ok_or_else(|| crate::Error::Other("missing cookie value after -b".into()))?;
                for (ck, cv) in parse_cookie_string(&cookie_str) {
                    req.cookies.insert(ck, cv);
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-urlencode" => {
                let data = iter
                    .next()
                    .ok_or_else(|| crate::Error::Other("missing data value".into()))?;
                if !explicit_method {
                    req.method = "POST".to_owned();
                }
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&data) {
                    if json_val.is_object() || json_val.is_array() {
                        req.json_data = Some(json_val);
                    } else {
                        req.data = Some(data);
                    }
                } else {
                    req.data = Some(data);
                }
            }
            "--json" => {
                let data = iter
                    .next()
                    .ok_or_else(|| crate::Error::Other("missing JSON value after --json".into()))?;
                if !explicit_method {
                    req.method = "POST".to_owned();
                }
                req.json_data = Some(serde_json::from_str(&data)?);
                req.headers
                    .entry("Content-Type".to_owned())
                    .or_insert_with(|| "application/json".to_owned());
            }
            "-x" | "--proxy" => {
                req.proxy =
                    Some(iter.next().ok_or_else(|| {
                        crate::Error::Other("missing proxy value after -x".into())
                    })?);
            }
            "-L" | "--location" => {
                req.follow_redirects = true;
            }
            "--compressed" | "-s" | "--silent" | "-S" | "--show-error" | "-k" | "--insecure" => {}
            s if s.starts_with("http://") || s.starts_with("https://") => {
                req.url = s.to_owned();
            }
            _ => {
                if !token.starts_with('-') && req.url.is_empty() {
                    req.url = token;
                }
            }
        }
    }

    if req.url.is_empty() {
        return Err(crate::Error::Other("no URL found in curl command".into()));
    }

    Ok(req)
}

/// Simple shell-like tokenizer that handles single/double quotes and backslash escapes.
fn shell_split(s: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;

    while let Some(c) = chars.next() {
        match c {
            '\\' if !in_single => {
                if let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\n' {
                        continue; // line continuation
                    }
                    current.push(next);
                }
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ' ' | '\t' | '\n' | '\r' if !in_single && !in_double => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if in_single || in_double {
        return Err("unterminated quote in curl command".to_owned());
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// HTML Convertor
// ---------------------------------------------------------------------------

/// HTML content conversion utilities.
pub struct Convertor;

impl Convertor {
    /// Convert HTML to Markdown.
    pub fn to_markdown(html: &str) -> String {
        let cleaned = Self::strip_noise_tags(html);
        html2md::parse_html(&cleaned)
    }

    /// Extract text content from HTML (no markup).
    pub fn to_text(html: &str) -> String {
        let cleaned = Self::strip_noise_tags(html);
        let doc = scraper::Html::parse_document(&cleaned);
        let mut text = String::new();
        for node in doc.root_element().descendants() {
            if let scraper::Node::Text(t) = node.value() {
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(trimmed);
                }
            }
        }
        text
    }

    /// Remove noise tags (script, style, noscript, svg) from HTML.
    fn strip_noise_tags(html: &str) -> String {
        let noise_tags = ["script", "style", "noscript", "svg"];
        let doc = scraper::Html::parse_document(html);
        let mut output = String::new();

        fn serialize_without_noise(
            node: ego_tree::NodeRef<scraper::Node>,
            noise: &[&str],
            out: &mut String,
        ) {
            match node.value() {
                scraper::Node::Document => {
                    for child in node.children() {
                        serialize_without_noise(child, noise, out);
                    }
                }
                scraper::Node::Element(el) => {
                    let tag = el.name();
                    if noise.contains(&tag) {
                        return;
                    }
                    out.push('<');
                    out.push_str(tag);
                    for (key, val) in el.attrs() {
                        out.push(' ');
                        out.push_str(key);
                        out.push_str("=\"");
                        out.push_str(&val.replace('"', "&quot;"));
                        out.push('"');
                    }
                    out.push('>');
                    for child in node.children() {
                        serialize_without_noise(child, noise, out);
                    }
                    out.push_str("</");
                    out.push_str(tag);
                    out.push('>');
                }
                scraper::Node::Text(t) => {
                    out.push_str(t);
                }
                scraper::Node::Comment(_) => {}
                _ => {}
            }
        }

        serialize_without_noise(doc.tree.root(), &noise_tags, &mut output);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cookie_string_basic() {
        let cookies = parse_cookie_string("name1=value1; name2=value2");
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0], ("name1".into(), "value1".into()));
        assert_eq!(cookies[1], ("name2".into(), "value2".into()));
    }

    #[test]
    fn parse_cookie_string_empty() {
        let cookies = parse_cookie_string("");
        assert!(cookies.is_empty());
    }

    #[test]
    fn parse_cookie_string_no_value() {
        let cookies = parse_cookie_string("flag");
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0], ("flag".into(), String::new()));
    }

    #[test]
    fn parse_headers_basic() {
        let lines = vec!["Content-Type: application/json", "Accept: */*"];
        let (headers, cookies) = parse_headers(&lines, true);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers["Content-Type"], "application/json");
        assert!(cookies.is_empty());
    }

    #[test]
    fn parse_headers_extracts_cookies() {
        let lines = vec!["Accept: text/html", "Cookie: session=abc; token=xyz"];
        let (headers, cookies) = parse_headers(&lines, true);
        assert_eq!(headers.len(), 1);
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies["session"], "abc");
        assert_eq!(cookies["token"], "xyz");
    }

    #[test]
    fn parse_headers_no_cookie_extraction() {
        let lines = vec!["Cookie: session=abc"];
        let (headers, cookies) = parse_headers(&lines, false);
        assert_eq!(headers.len(), 1);
        assert!(cookies.is_empty());
    }

    #[test]
    fn parse_curl_simple_get() {
        let req = parse_curl("curl https://example.com").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://example.com");
    }

    #[test]
    fn parse_curl_post_with_data() {
        let req =
            parse_curl(r#"curl -X POST https://api.example.com -d '{"key":"value"}'"#).unwrap();
        assert_eq!(req.method, "POST");
        assert!(req.json_data.is_some());
    }

    #[test]
    fn parse_curl_with_headers_and_cookies() {
        let req = parse_curl(
            r#"curl 'https://example.com' -H 'Accept: text/html' -H 'Cookie: sid=123; token=abc' -H 'User-Agent: test'"#,
        )
        .unwrap();
        assert_eq!(req.headers["Accept"], "text/html");
        assert_eq!(req.headers["User-Agent"], "test");
        assert_eq!(req.cookies["sid"], "123");
        assert_eq!(req.cookies["token"], "abc");
    }

    #[test]
    fn parse_curl_implicit_post() {
        let req = parse_curl("curl https://example.com -d 'field=value'").unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.data.as_deref(), Some("field=value"));
    }

    #[test]
    fn parse_curl_with_proxy() {
        let req = parse_curl("curl -x http://proxy:8080 https://example.com").unwrap();
        assert_eq!(req.proxy.as_deref(), Some("http://proxy:8080"));
    }

    #[test]
    fn parse_curl_follow_redirects() {
        let req = parse_curl("curl -L https://example.com").unwrap();
        assert!(req.follow_redirects);
    }

    #[test]
    fn parse_curl_no_url_error() {
        let result = parse_curl("curl -H 'Accept: text/html'");
        assert!(result.is_err());
    }

    #[test]
    fn convertor_to_text() {
        let html =
            "<html><body><h1>Hello</h1><script>alert('x')</script><p>World</p></body></html>";
        let text = Convertor::to_text(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("alert"));
    }

    #[test]
    fn convertor_to_markdown() {
        let html = "<html><body><h1>Title</h1><p>Paragraph</p></body></html>";
        let md = Convertor::to_markdown(html);
        assert!(md.contains("Title"));
        assert!(md.contains("Paragraph"));
    }

    #[test]
    fn convertor_strips_noise_tags() {
        let html = r#"<html><body><p>Keep</p><style>.x{}</style><script>evil()</script><noscript>no</noscript></body></html>"#;
        let text = Convertor::to_text(html);
        assert!(text.contains("Keep"));
        assert!(!text.contains("evil"));
        assert!(!text.contains(".x{}"));
    }

    #[test]
    fn shell_split_basic() {
        let tokens = shell_split("curl https://example.com -H 'Accept: text/html'").unwrap();
        assert_eq!(
            tokens,
            vec!["curl", "https://example.com", "-H", "Accept: text/html"]
        );
    }

    #[test]
    fn shell_split_double_quotes() {
        let tokens = shell_split(r#"curl "https://example.com" -d "hello world""#).unwrap();
        assert_eq!(
            tokens,
            vec!["curl", "https://example.com", "-d", "hello world"]
        );
    }

    #[test]
    fn shell_split_line_continuation() {
        let tokens = shell_split("curl \\\nhttps://example.com").unwrap();
        assert_eq!(tokens, vec!["curl", "https://example.com"]);
    }
}
