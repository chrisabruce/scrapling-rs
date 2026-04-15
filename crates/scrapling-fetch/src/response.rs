//! HTTP response type with lazy HTML parsing.
//!
//! The [`Response`] struct is what you get back from every request made through
//! [`Fetcher`](crate::Fetcher) or [`FetcherSession`](crate::FetcherSession). It holds
//! the raw response bytes, headers, cookies, status code, and metadata. The HTML body
//! is not parsed until you first call [`selector()`](Response::selector), [`css()`](Response::css),
//! or any other method that needs the DOM -- this keeps simple status-code checks fast.
//!
//! The internal [`build_response_async`] function converts a raw `wreq::Response` into
//! this type and is used by the client module.

use std::cell::OnceCell;
use std::collections::HashMap;

use bytes::Bytes;
use serde_json::Value;

use scrapling::selector::Selector;

use crate::status::status_text;

/// HTTP response with lazy-parsed HTML selector.
///
/// The response body is stored as raw bytes. The HTML [`Selector`] is parsed
/// lazily on first access via [`selector()`](Response::selector). This means
/// creating and inspecting a `Response` (checking status, reading headers) is
/// cheap -- the potentially expensive HTML parse only happens when you need the DOM.
///
/// `Response` implements `Send` so it can be moved across threads, but the lazy
/// [`OnceCell`] storing the parsed selector uses interior mutability, so it is not
/// `Sync`. Parse on one thread, then share the results.
pub struct Response {
    /// The HTTP status code.
    pub status: u16,
    /// The reason phrase for the status code.
    pub reason: String,
    /// Cookies received in the response.
    pub cookies: HashMap<String, String>,
    /// Response headers.
    pub headers: HashMap<String, String>,
    /// Headers that were sent with the request.
    pub request_headers: HashMap<String, String>,
    /// Redirect history leading to this response.
    pub history: Vec<Response>,
    /// The character encoding of the response body.
    pub encoding: String,
    /// The HTTP method used for the request.
    pub method: String,
    /// Arbitrary metadata associated with this response.
    pub meta: HashMap<String, Value>,
    /// The raw response body bytes.
    pub body: Bytes,
    url: String,
    parsed: OnceCell<Selector>,
}

unsafe impl Send for Response {}

impl Response {
    /// Creates a new response from its constituent parts. This is primarily used
    /// internally by [`build_response_async`]. Most callers will receive `Response`
    /// objects from [`Fetcher::get()`](crate::Fetcher::get) and similar methods.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: &str,
        body: Bytes,
        status: u16,
        reason: Option<String>,
        cookies: HashMap<String, String>,
        headers: HashMap<String, String>,
        request_headers: HashMap<String, String>,
        encoding: String,
        method: String,
        history: Vec<Response>,
        meta: HashMap<String, Value>,
    ) -> Self {
        Self {
            status,
            reason: reason.unwrap_or_else(|| status_text(status).to_owned()),
            cookies,
            headers,
            request_headers,
            history,
            encoding,
            method,
            meta,
            body,
            url: url.to_owned(),
            parsed: OnceCell::new(),
        }
    }

    /// Returns the final URL of the response after any redirects have been followed.
    /// If no redirects occurred, this is the same as the original request URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the parsed HTML selector, parsing the body on first call.
    ///
    /// The parse result is cached, so subsequent calls return immediately. The body
    /// is decoded as UTF-8 (with lossy replacement for invalid sequences) before parsing.
    pub fn selector(&self) -> &Selector {
        self.parsed.get_or_init(|| {
            let html = String::from_utf8_lossy(&self.body);
            Selector::from_html_with_url(&html, &self.url)
        })
    }

    /// Runs a CSS selector query against the parsed HTML and returns matching elements.
    /// This is a convenience wrapper around `self.selector().css(query)`. Triggers a
    /// lazy parse if the body has not been parsed yet.
    pub fn css(&self, query: &str) -> scrapling::selector::Selectors {
        self.selector().css(query)
    }

    /// Finds elements whose text content matches the given string. Use `partial` for
    /// substring matching, `case_sensitive` to control case, and `clean_match` to
    /// strip whitespace before comparing.
    pub fn find_by_text(
        &self,
        text: &str,
        partial: bool,
        case_sensitive: bool,
        clean_match: bool,
    ) -> scrapling::selector::Selectors {
        self.selector()
            .find_by_text(text, partial, case_sensitive, clean_match)
    }

    /// Returns a text handler for extracting and manipulating text content from the
    /// parsed HTML. Useful for getting the full visible text of the page.
    pub fn text(&self) -> scrapling::TextHandler {
        self.selector().text()
    }

    /// Resolves a relative URL against this response's base URL. For example, if the
    /// response URL is `https://example.com/page` and you pass `"/other"`, this returns
    /// `"https://example.com/other"`.
    pub fn urljoin(&self, relative: &str) -> String {
        self.selector().urljoin(relative)
    }

    /// Returns `true` if the status code is in the 2xx range (200-299), indicating
    /// the request was successfully received, understood, and accepted.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Returns `true` if the status code is in the 3xx range (300-399). You will only
    /// see this when redirect following is disabled, since otherwise the client follows
    /// redirects automatically.
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    /// Returns `true` if the status code is in the 4xx range (400-499), indicating
    /// a client error such as a bad request, unauthorized access, or not found.
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Returns `true` if the status code is in the 5xx range (500-599), indicating
    /// a server-side error. These are often transient and may succeed on retry.
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Resolves a relative URL for following a link. This is a semantic alias for
    /// [`urljoin()`](Self::urljoin) that makes crawler code read more naturally.
    pub fn follow_url(&self, relative: &str) -> String {
        self.urljoin(relative)
    }

    /// Converts the HTML body to Markdown using the scrapling shell converter. Useful
    /// for feeding page content to LLMs or for human-readable text extraction.
    pub fn to_markdown(&self) -> String {
        scrapling::shell::Convertor::to_markdown(&String::from_utf8_lossy(&self.body))
    }

    /// Converts the HTML body to plain text, stripping all HTML tags and formatting.
    /// Useful when you only care about the visible text content of a page.
    pub fn to_text(&self) -> String {
        scrapling::shell::Convertor::to_text(&String::from_utf8_lossy(&self.body))
    }
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("url", &self.url)
            .field("method", &self.method)
            .finish()
    }
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{} {}>", self.status, self.url)
    }
}

fn extract_encoding(headers: &HashMap<String, String>) -> String {
    headers
        .get("content-type")
        .and_then(|ct| {
            ct.split(';').find_map(|part| {
                part.trim()
                    .strip_prefix("charset=")
                    .map(|c| c.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "utf-8".to_owned())
}

/// Builds a [`Response`] from a raw `wreq::Response` by extracting headers, cookies,
/// status, encoding, and downloading the body bytes. This is called internally by
/// [`Fetcher`](crate::Fetcher) and [`FetcherSession`](crate::FetcherSession) after
/// each successful HTTP exchange.
pub(crate) async fn build_response_async(
    resp: wreq::Response,
    request_headers: HashMap<String, String>,
    method: &str,
    meta: HashMap<String, Value>,
) -> crate::error::Result<Response> {
    let status = resp.status().as_u16();
    let reason = resp.status().canonical_reason().map(|s| s.to_owned());

    let headers: HashMap<String, String> = resp
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_owned(), v.to_owned()))
        })
        .collect();

    let cookies: HashMap<String, String> = resp
        .cookies()
        .map(|c| (c.name().to_owned(), c.value().to_owned()))
        .collect();

    let encoding = extract_encoding(&headers);
    let url = resp.uri().to_string();
    let body_bytes = resp.bytes().await?;

    Ok(Response::new(
        &url,
        body_bytes,
        status,
        reason,
        cookies,
        headers,
        request_headers,
        encoding,
        method.to_owned(),
        Vec::new(),
        meta,
    ))
}
