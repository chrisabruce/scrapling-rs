//! Proxy configuration and rotation for HTTP requests.
//!
//! When scraping at scale, routing requests through proxy servers helps avoid IP-based
//! rate limiting and bans. This module provides two mechanisms:
//!
//! - **Static proxy** -- A single [`Proxy`] set on [`FetcherConfig`](crate::FetcherConfig)
//!   that is used for every request.
//! - **Proxy rotation** -- A [`ProxyRotator`] that cycles through a pool of proxies,
//!   picking the next one for each request according to a [`RotationStrategy`].
//!
//! The [`is_proxy_error`] helper function inspects error messages to determine whether
//! a failure was proxy-related, which is useful for deciding whether to retry with a
//! different proxy.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::error::FetchError;

/// A proxy server specification, either as a URL string or a structured configuration.
///
/// The two variants exist for serialization convenience: simple proxies can be written
/// as a plain URL string in config files, while proxies with credentials use the
/// structured `Config` variant with explicit fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Proxy {
    /// A proxy specified as a plain URL string, e.g. `"http://proxy.example.com:8080"`.
    /// If the proxy requires authentication, embed credentials in the URL.
    Url(String),
    /// A proxy specified with explicit server, username, and password fields. This is
    /// cleaner than embedding credentials in the URL and works well in config files.
    Config {
        /// The proxy server address (e.g., `"http://proxy.example.com:8080"`).
        server: String,
        /// Optional authentication username.
        #[serde(default)]
        username: Option<String>,
        /// Optional authentication password.
        #[serde(default)]
        password: Option<String>,
    },
}

impl Proxy {
    fn key(&self) -> String {
        match self {
            Self::Url(url) => url.clone(),
            Self::Config {
                server, username, ..
            } => {
                let user = username.as_deref().unwrap_or("");
                format!("{server}|{user}")
            }
        }
    }

    /// Returns the proxy server address as a string slice. For `Url` variants this
    /// is the full URL; for `Config` variants this is the `server` field.
    pub fn server(&self) -> &str {
        match self {
            Self::Url(url) => url.as_str(),
            Self::Config { server, .. } => server.as_str(),
        }
    }
}

impl std::fmt::Display for Proxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.server())
    }
}

/// A function that determines the next proxy index given the proxy list and the current
/// index. Implement your own to create custom rotation strategies (e.g., random,
/// weighted, or geo-aware selection).
pub type RotationStrategy = fn(&[Proxy], usize) -> usize;

/// The default rotation strategy that cycles through proxies sequentially (0, 1, 2, ...,
/// then back to 0). The returned index is wrapped with modulo in [`ProxyRotator::get_proxy`],
/// so this simply returns `current + 1`.
pub fn cyclic_rotation(_proxies: &[Proxy], current: usize) -> usize {
    current + 1
}

const PROXY_ERROR_INDICATORS: &[&str] = &[
    "net::err_proxy",
    "net::err_tunnel",
    "connection refused",
    "connection reset",
    "connection timed out",
    "failed to connect",
    "could not resolve proxy",
];

/// Returns `true` if the error message indicates a proxy-related failure. This checks
/// for common proxy error patterns like "connection refused", "tunnel failed", etc.
/// Useful in retry logic to decide whether to switch to a different proxy.
pub fn is_proxy_error(error: &dyn std::error::Error) -> bool {
    let msg = error.to_string().to_lowercase();
    PROXY_ERROR_INDICATORS.iter().any(|ind| msg.contains(ind))
}

/// Thread-safe proxy rotator that cycles through a list of proxies using a configurable strategy.
///
/// The rotator holds a `Mutex`-protected index that advances each time [`get_proxy()`](Self::get_proxy)
/// is called. Duplicate proxies are rejected at construction time to prevent wasted cycles.
/// The default strategy is [`cyclic_rotation`], but you can supply any function matching
/// the [`RotationStrategy`] signature.
pub struct ProxyRotator {
    proxies: Vec<Proxy>,
    strategy: RotationStrategy,
    current_index: Mutex<usize>,
}

impl ProxyRotator {
    /// Creates a new rotator with the default [`cyclic_rotation`] strategy. Returns
    /// an error if the proxy list is empty or contains duplicates.
    pub fn new(proxies: Vec<Proxy>) -> crate::error::Result<Self> {
        Self::with_strategy(proxies, cyclic_rotation)
    }

    /// Creates a new rotator with a custom rotation strategy. The strategy function
    /// receives the full proxy list and the current index, and returns the next index.
    /// Returns an error if the proxy list is empty or contains duplicates.
    pub fn with_strategy(
        proxies: Vec<Proxy>,
        strategy: RotationStrategy,
    ) -> crate::error::Result<Self> {
        if proxies.is_empty() {
            return Err(FetchError::InvalidProxy(
                "at least one proxy must be provided".into(),
            ));
        }
        // Validate uniqueness
        let mut seen = std::collections::HashSet::new();
        for p in &proxies {
            let key = p.key();
            if !seen.insert(key.clone()) {
                return Err(FetchError::InvalidProxy(format!("duplicate proxy: {key}")));
            }
        }
        Ok(Self {
            proxies,
            strategy,
            current_index: Mutex::new(0),
        })
    }

    /// Returns the next proxy according to the rotation strategy and advances the
    /// internal index. The index is taken modulo the proxy count, so strategies can
    /// return any value without worrying about bounds.
    pub fn get_proxy(&self) -> Proxy {
        let mut idx = self.current_index.lock().unwrap();
        let actual = *idx % self.proxies.len();
        let proxy = self.proxies[actual].clone();
        *idx = (self.strategy)(&self.proxies, actual);
        proxy
    }

    /// Returns a slice of all configured proxies. Useful for logging or diagnostics.
    pub fn proxies(&self) -> &[Proxy] {
        &self.proxies
    }

    /// Returns the number of proxies in the rotator. Always at least 1 since empty
    /// proxy lists are rejected at construction time.
    pub fn len(&self) -> usize {
        self.proxies.len()
    }

    /// Returns `true` if the rotator contains no proxies. In practice this always
    /// returns `false` since the constructor requires at least one proxy, but this
    /// method is provided for API completeness alongside [`len()`](Self::len).
    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }
}

impl std::fmt::Debug for ProxyRotator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyRotator")
            .field("count", &self.proxies.len())
            .finish()
    }
}
