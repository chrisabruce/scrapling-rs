//! Thread-safe log-level counter for crawl diagnostics.
//!
//! The [`LogCounter`] keeps a running tally of log messages by level (debug,
//! info, warning, error). It uses atomic integers so it can be safely shared
//! across threads without a mutex.
//!
//! After the crawl completes, call [`LogCounter::counts`] and assign the
//! result to [`CrawlStats::log_levels_counter`](crate::result::CrawlStats::log_levels_counter)
//! to include log-level breakdowns in the final statistics.
//!
//! This is the Rust equivalent of the Python scrapling's `LogCounterHandler`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Counts log events by level for post-crawl diagnostics.
///
/// Thread-safe via `Arc<AtomicU64>` counters -- you can clone this struct and
/// share it across threads or async tasks without synchronization concerns.
/// Feed it log events via [`increment`](LogCounter::increment) and retrieve
/// the final tallies via [`counts`](LogCounter::counts).
#[derive(Debug, Default, Clone)]
pub struct LogCounter {
    debug: Arc<AtomicU64>,
    info: Arc<AtomicU64>,
    warn: Arc<AtomicU64>,
    error: Arc<AtomicU64>,
}

impl LogCounter {
    /// Creates a new counter with all levels at zero. This is equivalent to
    /// `LogCounter::default()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomically increments the counter for the given log level. `TRACE`
    /// events are grouped with `DEBUG` since they serve a similar diagnostic
    /// purpose. This method uses `Relaxed` ordering because exact accuracy
    /// is not critical for aggregate counters.
    pub fn increment(&self, level: tracing::Level) {
        match level {
            tracing::Level::DEBUG | tracing::Level::TRACE => {
                self.debug.fetch_add(1, Ordering::Relaxed);
            }
            tracing::Level::INFO => {
                self.info.fetch_add(1, Ordering::Relaxed);
            }
            tracing::Level::WARN => {
                self.warn.fetch_add(1, Ordering::Relaxed);
            }
            tracing::Level::ERROR => {
                self.error.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Returns a snapshot of all level counts as a string-keyed map. The keys
    /// are `"debug"`, `"info"`, `"warning"`, and `"error"`. Assign the
    /// returned map to [`CrawlStats::log_levels_counter`](crate::result::CrawlStats::log_levels_counter)
    /// to include log statistics in the crawl output.
    pub fn counts(&self) -> HashMap<String, u64> {
        HashMap::from([
            ("debug".into(), self.debug.load(Ordering::Relaxed)),
            ("info".into(), self.info.load(Ordering::Relaxed)),
            ("warning".into(), self.warn.load(Ordering::Relaxed)),
            ("error".into(), self.error.load(Ordering::Relaxed)),
        ])
    }
}
