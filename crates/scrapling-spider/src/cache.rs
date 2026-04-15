//! Filesystem-based HTTP response cache for development mode.
//!
//! When [`Spider::development_mode`](crate::spider::Spider::development_mode)
//! returns `true`, the engine creates a [`ResponseCacheManager`] that saves
//! every fetched response to disk as a JSON file. On subsequent runs, cached
//! responses are served directly without hitting the network, which dramatically
//! speeds up iterative development of parse logic.
//!
//! Each cached response is stored as `<hex-fingerprint>.json` inside the cache
//! directory. The response body is base64-encoded to keep the JSON valid
//! regardless of the body's content type. Writes are atomic (temp file + rename)
//! to prevent partial files.
//!
//! This cache is not intended for production use. It does not implement
//! expiration, size limits, or cache-control header semantics.

use std::collections::HashMap;
use std::path::PathBuf;

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use scrapling_fetch::Response;

use crate::error::{Result, SpiderError};

/// Manages a filesystem cache of HTTP responses, keyed by request fingerprint.
///
/// The cache stores each response as a JSON file named after the hex-encoded
/// SHA-1 fingerprint of the request that produced it. This is only active when
/// the spider has [`development_mode`](crate::spider::Spider::development_mode)
/// enabled; production crawls skip the cache entirely.
pub struct ResponseCacheManager {
    cache_dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct CachedResponse {
    url: String,
    status: u16,
    reason: String,
    encoding: String,
    cookies: HashMap<String, String>,
    headers: HashMap<String, String>,
    request_headers: HashMap<String, String>,
    method: String,
    content_base64: String,
}

impl ResponseCacheManager {
    /// Creates a new cache manager that stores entries in the given directory.
    /// The directory is created lazily on the first [`put`](ResponseCacheManager::put)
    /// call, so it does not need to exist at construction time.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
        }
    }

    fn cache_path(&self, fingerprint: &[u8]) -> PathBuf {
        self.cache_dir
            .join(format!("{}.json", hex::encode(fingerprint)))
    }

    /// Retrieves a cached response by its fingerprint, or `None` if not found.
    ///
    /// The method reads the JSON file, deserializes the cached fields, and
    /// base64-decodes the response body. If any step fails (missing file,
    /// corrupt JSON, invalid base64), a warning is logged and `None` is
    /// returned so the engine falls through to a live fetch.
    pub fn get(&self, fingerprint: &[u8]) -> Option<Response> {
        use base64::Engine;

        let path = self.cache_path(fingerprint);
        let data = std::fs::read(&path)
            .inspect_err(|e| warn!(error = %e, "failed to read cache file"))
            .ok()?;

        let cached: CachedResponse = serde_json::from_slice(&data)
            .inspect_err(|e| warn!(error = %e, "failed to deserialize cache entry"))
            .ok()?;

        let body = base64::engine::general_purpose::STANDARD
            .decode(&cached.content_base64)
            .inspect_err(|e| warn!(error = %e, "failed to decode cached body"))
            .ok()
            .map(Bytes::from)?;

        Some(Response::new(
            &cached.url,
            body,
            cached.status,
            Some(cached.reason),
            cached.cookies,
            cached.headers,
            cached.request_headers,
            cached.encoding,
            cached.method,
            Vec::new(),
            HashMap::new(),
        ))
    }

    /// Stores a response in the cache under the given fingerprint.
    ///
    /// The response body is base64-encoded and the entire entry is serialized
    /// to JSON. The write is atomic: data goes to a temp file first, then is
    /// renamed to the final path, so a crash mid-write cannot corrupt an
    /// existing cache entry.
    pub fn put(&self, fingerprint: &[u8], response: &Response, method: &str) -> Result<()> {
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|e| SpiderError::Other(format!("failed to create cache dir: {e}")))?;

        use base64::Engine;
        let content_base64 = base64::engine::general_purpose::STANDARD.encode(&response.body);

        let cached = CachedResponse {
            url: response.url().to_owned(),
            status: response.status,
            reason: response.reason.clone(),
            encoding: response.encoding.clone(),
            cookies: response.cookies.clone(),
            headers: response.headers.clone(),
            request_headers: response.request_headers.clone(),
            method: method.to_owned(),
            content_base64,
        };

        let temp_path = self.cache_dir.join(".cache.tmp");
        let json = serde_json::to_vec(&cached)
            .map_err(|e| SpiderError::Other(format!("cache serialization failed: {e}")))?;

        std::fs::write(&temp_path, &json)
            .map_err(|e| SpiderError::Other(format!("failed to write cache: {e}")))?;

        let target = self.cache_path(fingerprint);
        std::fs::rename(&temp_path, &target).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            SpiderError::Other(format!("failed to rename cache file: {e}"))
        })?;

        debug!("response cached");
        Ok(())
    }

    /// Removes all cached response files (`.json`) from the cache directory.
    /// Call this when you want to force a fresh crawl during development. Only
    /// files with a `.json` extension are deleted; other files in the directory
    /// are left untouched.
    pub fn clear(&self) -> Result<()> {
        if self.cache_dir.exists() {
            for entry in std::fs::read_dir(&self.cache_dir)
                .map_err(|e| SpiderError::Other(format!("failed to read cache dir: {e}")))?
                .flatten()
            {
                if entry.path().extension().is_some_and(|e| e == "json") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(())
    }
}
