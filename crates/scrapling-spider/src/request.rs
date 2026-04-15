//! Request and callback types for the spider crawl pipeline.
//!
//! This module defines the core data that flows through the crawler:
//!
//! - [`Request`] -- a URL to fetch, together with scheduling priority, session
//!   routing, deduplication fingerprint, retry state, and an optional callback.
//! - [`Callback`] -- a boxed closure that turns a fetched [`Response`] into zero
//!   or more [`SpiderOutput`] values.
//! - [`SpiderOutput`] -- the two things a callback can produce: a scraped data
//!   item (`Item`) or a follow-up request (`FollowRequest`).
//!
//! Requests use a builder pattern (`Request::new(url).with_priority(10)`) and are
//! ordered by priority so the [`Scheduler`](crate::scheduler::Scheduler) always
//! processes the most important URLs first.

use std::cmp::Ordering;
use std::collections::HashMap;

use sha1::{Digest, Sha1};

use scrapling_fetch::Response;

/// A boxed closure that processes an HTTP response and returns spider outputs.
///
/// Callbacks are attached to individual [`Request`]s via
/// [`Request::with_callback`]. When the crawler receives a response for that
/// request, it invokes the callback instead of the spider's default `parse`
/// method. This lets you route different pages to different parsing logic.
pub type Callback = Box<dyn Fn(Response) -> Vec<SpiderOutput> + Send + Sync>;

/// The result of processing a response: either a scraped data item or a
/// follow-up request to enqueue.
///
/// Your [`Spider::parse`](crate::spider::Spider::parse) implementation (or a
/// per-request [`Callback`]) returns a `Vec<SpiderOutput>`. The crawler engine
/// collects `Item` values into the final [`ItemList`](crate::result::ItemList)
/// and feeds `FollowRequest` values back into the [`Scheduler`](crate::scheduler::Scheduler).
#[derive(Debug)]
pub enum SpiderOutput {
    /// A scraped data item to be collected. The JSON value is passed through the
    /// spider's `on_scraped_item` hook before being stored, which gives you a
    /// chance to validate, transform, or drop it.
    Item(serde_json::Value),
    /// A new request to enqueue for crawling. The engine checks domain
    /// restrictions and deduplication before actually scheduling it.
    FollowRequest(Request),
}

/// A crawl request with URL, priority, metadata, and optional callback.
///
/// `Request` is the unit of work in the crawl pipeline. Create one with
/// [`Request::new`], customize it with the builder methods (`with_priority`,
/// `with_sid`, `with_callback`, etc.), and return it from your spider's `parse`
/// method wrapped in [`SpiderOutput::FollowRequest`].
///
/// Two requests are considered equal if their fingerprints match (or, if no
/// fingerprint has been computed, if their URLs match). Ordering is by priority
/// (higher values are dequeued first).
pub struct Request {
    /// The URL to fetch. This is the only required field; everything else has
    /// sensible defaults set by [`Request::new`].
    pub url: String,
    /// The session identifier used to select a fetcher from the
    /// [`SessionManager`](crate::session::SessionManager). An empty string
    /// means "use the default session."
    pub sid: String,
    /// An optional callback to process the response. When present, the engine
    /// calls this closure instead of [`Spider::parse`](crate::spider::Spider::parse).
    /// Because closures are not cloneable, use [`copy_without_callback`](Request::copy_without_callback)
    /// when you need to duplicate a request for retries.
    pub callback: Option<Callback>,
    /// The name of the callback, kept for debugging output and checkpoint
    /// serialization. It has no effect on routing; the actual closure in
    /// `callback` is what gets invoked.
    pub callback_name: Option<String>,
    /// The scheduling priority. Higher values are dequeued first by the
    /// [`Scheduler`](crate::scheduler::Scheduler). The default is 0. Use
    /// negative values to de-prioritize retries or background pages.
    pub priority: i32,
    /// Whether to bypass the duplicate-request filter. Set this to `true` when
    /// you intentionally want to re-fetch a URL -- for example, to poll a page
    /// for updates or to retry after a transient failure.
    pub dont_filter: bool,
    /// Arbitrary metadata passed through the crawl pipeline. Whatever you put
    /// here is available on the request when it reaches your callback, which is
    /// useful for carrying context (e.g., a parent-page ID) between parse stages.
    pub meta: HashMap<String, serde_json::Value>,
    /// The number of times this request has been retried after receiving a
    /// blocked response. The engine increments this automatically and stops
    /// retrying once it exceeds [`Spider::max_blocked_retries`](crate::spider::Spider::max_blocked_retries).
    pub retry_count: u32,
    /// Additional keyword arguments forwarded to the session fetcher. Common
    /// keys include `"method"`, `"headers"`, `"data"`, and `"json"`. These are
    /// also factored into the deduplication fingerprint when
    /// [`Spider::fp_include_kwargs`](crate::spider::Spider::fp_include_kwargs) is enabled.
    pub session_kwargs: HashMap<String, serde_json::Value>,
    fingerprint: Option<Vec<u8>>,
}

impl Request {
    /// Creates a new request for the given URL with default settings.
    ///
    /// All fields are initialized to their zero/empty values: priority 0, no
    /// session override, no callback, no metadata, and duplicate filtering
    /// enabled. Use the `with_*` builder methods to customize.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            sid: String::new(),
            callback: None,
            callback_name: None,
            priority: 0,
            dont_filter: false,
            meta: HashMap::new(),
            retry_count: 0,
            session_kwargs: HashMap::new(),
            fingerprint: None,
        }
    }

    /// Sets the session identifier for this request, routing it to a specific
    /// fetcher registered in the [`SessionManager`](crate::session::SessionManager).
    /// Use this when your spider manages multiple sessions with different cookies,
    /// proxies, or authentication contexts.
    pub fn with_sid(mut self, sid: impl Into<String>) -> Self {
        self.sid = sid.into();
        self
    }

    /// Sets the scheduling priority for this request. Higher values are
    /// dequeued first. Use positive values for important pages (e.g., product
    /// detail pages) and negative values for low-priority background work.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Sets whether this request should bypass the duplicate-request filter.
    /// Pass `true` to allow re-fetching a URL that has already been seen. This
    /// is useful for polling pages that change over time or for manual retries.
    pub fn with_dont_filter(mut self, dont_filter: bool) -> Self {
        self.dont_filter = dont_filter;
        self
    }

    /// Attaches arbitrary metadata to this request. The metadata map is carried
    /// through the entire crawl pipeline and is accessible in your callback or
    /// `parse` implementation, making it the standard way to pass context (such
    /// as a parent URL or category label) between crawl stages.
    pub fn with_meta(mut self, meta: HashMap<String, serde_json::Value>) -> Self {
        self.meta = meta;
        self
    }

    /// Attaches a named callback to process the response for this request.
    /// When the engine receives the response, it will call this closure instead
    /// of [`Spider::parse`](crate::spider::Spider::parse). The `name` is stored
    /// for debugging and checkpoint serialization; it does not affect dispatch.
    pub fn with_callback(mut self, name: &str, callback: Callback) -> Self {
        self.callback_name = Some(name.to_owned());
        self.callback = Some(callback);
        self
    }

    /// Extracts the domain (host) from the request URL. Returns an empty
    /// string if the URL cannot be parsed. This is used internally for domain
    /// allowlisting and per-domain statistics, but you can also call it in your
    /// own code to inspect which host a request targets.
    pub fn domain(&self) -> String {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_owned()))
            .unwrap_or_default()
    }

    /// Computes and caches a SHA-1 fingerprint for deduplication, returning it
    /// as a byte slice.
    ///
    /// The fingerprint is derived from the session ID, HTTP method, URL, and
    /// request body. The boolean flags control whether session kwargs, headers,
    /// and URL fragments are also included. Once computed, the fingerprint is
    /// cached so subsequent calls are free. The [`Scheduler`](crate::scheduler::Scheduler)
    /// calls this automatically when a request is enqueued.
    pub fn update_fingerprint(
        &mut self,
        include_kwargs: bool,
        include_headers: bool,
        keep_fragments: bool,
    ) -> &[u8] {
        if let Some(ref fp) = self.fingerprint {
            return fp;
        }

        let mut url = self.url.clone();
        if !keep_fragments {
            if let Some(pos) = url.find('#') {
                url.truncate(pos);
            }
        }

        let method = self
            .session_kwargs
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();

        let body = self.extract_body_hex();

        let mut parts = serde_json::Map::new();
        parts.insert("sid".into(), serde_json::Value::String(self.sid.clone()));
        parts.insert("method".into(), serde_json::Value::String(method));
        parts.insert("url".into(), serde_json::Value::String(url));
        parts.insert("body".into(), serde_json::Value::String(body));

        if include_kwargs {
            let mut keys: Vec<&String> = self.session_kwargs.keys().collect();
            keys.sort();
            let hex = hex::encode(format!("{keys:?}"));
            parts.insert("kwargs".into(), serde_json::Value::String(hex));
        }

        if include_headers {
            if let Some(headers) = self.session_kwargs.get("headers") {
                let s = serde_json::to_string(headers).unwrap_or_default();
                parts.insert("headers".into(), serde_json::Value::String(s));
            }
        }

        let serialized = serde_json::to_vec(&parts).unwrap_or_default();
        let mut hasher = Sha1::new();
        hasher.update(&serialized);
        let fp = hasher.finalize().to_vec();
        self.fingerprint = Some(fp);
        self.fingerprint.as_ref().unwrap()
    }

    fn extract_body_hex(&self) -> String {
        if let Some(data) = self.session_kwargs.get("data") {
            if let Some(s) = data.as_str() {
                return hex::encode(s.as_bytes());
            }
            return hex::encode(serde_json::to_vec(data).unwrap_or_default());
        }
        if let Some(json) = self.session_kwargs.get("json") {
            return hex::encode(serde_json::to_vec(json).unwrap_or_default());
        }
        String::new()
    }

    /// Returns the cached fingerprint, if one has been computed via
    /// [`update_fingerprint`](Request::update_fingerprint). Returns `None` if
    /// the fingerprint has never been calculated. The cache manager uses this to
    /// look up previously stored responses.
    pub fn fingerprint(&self) -> Option<&[u8]> {
        self.fingerprint.as_deref()
    }

    /// Creates a clone of this request without the callback closure.
    ///
    /// Because [`Callback`] is a boxed `dyn Fn` and cannot be cloned, this
    /// method copies every field except `callback` (which is set to `None`).
    /// The engine uses this when creating retry requests for blocked responses.
    pub fn copy_without_callback(&self) -> Self {
        Self {
            url: self.url.clone(),
            sid: self.sid.clone(),
            callback: None,
            callback_name: self.callback_name.clone(),
            priority: self.priority,
            dont_filter: self.dont_filter,
            meta: self.meta.clone(),
            retry_count: self.retry_count,
            session_kwargs: self.session_kwargs.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("url", &self.url)
            .field("priority", &self.priority)
            .field("callback", &self.callback_name)
            .finish()
    }
}

impl std::fmt::Display for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}

impl PartialEq for Request {
    fn eq(&self, other: &Self) -> bool {
        match (&self.fingerprint, &other.fingerprint) {
            (Some(a), Some(b)) => a == b,
            _ => self.url == other.url,
        }
    }
}

impl Eq for Request {}

impl PartialOrd for Request {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Request {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}
