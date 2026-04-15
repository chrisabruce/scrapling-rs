//! Thread-safe browser page pool for tracking concurrent page usage.
//!
//! When a session is configured with `max_pages > 1`, multiple pages can be open
//! simultaneously. The [`PagePool`] keeps track of how many pages exist, which ones
//! are busy, and enforces the capacity limit so the browser does not consume
//! unbounded resources.
//!
//! Each page is identified by a zero-based `page_index` and transitions through
//! three states defined by [`PageState`]:
//!
//! ```text
//!   Ready ──▶ Busy ──▶ Ready   (successful navigation)
//!                  └──▶ Error   (unrecoverable failure)
//! ```
//!
//! Pages in the `Error` state can be cleaned up with [`PagePool::cleanup_error_pages`].
//! The [`PoolStats`] snapshot is useful for monitoring and logging.

use std::sync::Mutex;

/// Lifecycle state of a pooled browser page.
///
/// Pages start in `Ready`, transition to `Busy` during navigation, and return to
/// `Ready` on success or move to `Error` on unrecoverable failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageState {
    /// The page is idle and available for a new navigation.
    /// A page returns to this state after a successful fetch cycle.
    Ready,

    /// The page is currently performing a navigation or action.
    /// While in this state the page should not be reused by another fetch.
    Busy,

    /// The page encountered an unrecoverable error and should be discarded.
    /// Call [`PagePool::cleanup_error_pages`] to remove all errored pages.
    Error,
}

/// Metadata tracked for each page in the pool.
///
/// Stores the page's index, current lifecycle state, and the URL it is navigating
/// to (if any). The pool uses this information to enforce capacity limits and to
/// report statistics via [`PoolStats`].
#[derive(Debug)]
pub struct PageInfo {
    /// Zero-based index identifying this page within the pool.
    pub page_index: usize,
    /// Current lifecycle state of the page.
    pub state: PageState,
    /// URL the page is currently navigated to, or empty if idle.
    pub url: String,
}

impl PageInfo {
    /// Create a new `PageInfo` in the `Ready` state with the given index.
    /// The URL is initially empty and will be set when the page starts navigating.
    pub fn new(page_index: usize) -> Self {
        Self {
            page_index,
            state: PageState::Ready,
            url: String::new(),
        }
    }
}

/// Thread-safe pool that tracks browser page states and enforces a capacity limit.
///
/// The pool uses a `Mutex<Vec<PageInfo>>` internally, making it safe to share across
/// async tasks. All mutation methods acquire the lock, perform the update, and release
/// it immediately to minimise contention.
pub struct PagePool {
    /// Maximum number of pages allowed in the pool.
    pub max_pages: u32,
    pages: Mutex<Vec<PageInfo>>,
}

impl PagePool {
    /// Create an empty page pool with the given capacity.
    /// No pages are registered yet -- call [`add_page`](Self::add_page) to register
    /// each new page as it is created by the session.
    pub fn new(max_pages: u32) -> Self {
        Self {
            max_pages,
            pages: Mutex::new(Vec::new()),
        }
    }

    /// Register a new page in the pool, returning an error if the pool is full.
    /// The page starts in the [`PageState::Ready`] state. Returns
    /// [`BrowserError::PagePool`] when the number of registered pages already
    /// equals `max_pages`.
    pub fn add_page(&self, page_index: usize) -> crate::error::Result<()> {
        let mut pages = self.pages.lock().unwrap();
        if pages.len() >= self.max_pages as usize {
            return Err(crate::error::BrowserError::PagePool(format!(
                "page pool full ({}/{})",
                pages.len(),
                self.max_pages
            )));
        }
        pages.push(PageInfo::new(page_index));
        Ok(())
    }

    fn with_page(&self, page_index: usize, f: impl FnOnce(&mut PageInfo)) {
        let mut pages = self.pages.lock().unwrap();
        if let Some(info) = pages.iter_mut().find(|p| p.page_index == page_index) {
            f(info);
        }
    }

    /// Mark a page as busy and record the URL it is navigating to.
    /// Call this at the start of a fetch cycle to signal that the page is in use.
    pub fn mark_busy(&self, page_index: usize, url: &str) {
        self.with_page(page_index, |info| {
            info.state = PageState::Busy;
            info.url = url.to_owned();
        });
    }

    /// Mark a page as ready (idle) for reuse.
    /// Call this after a fetch cycle completes successfully.
    pub fn mark_ready(&self, page_index: usize) {
        self.with_page(page_index, |info| info.state = PageState::Ready);
    }

    /// Mark a page as having encountered an error.
    /// The page will not be reused until it is cleaned up via
    /// [`cleanup_error_pages`](Self::cleanup_error_pages).
    pub fn mark_error(&self, page_index: usize) {
        self.with_page(page_index, |info| info.state = PageState::Error);
    }

    /// Return the total number of pages currently in the pool (all states combined).
    pub fn pages_count(&self) -> usize {
        self.pages.lock().unwrap().len()
    }

    /// Return the number of pages currently in the `Busy` state.
    pub fn busy_count(&self) -> usize {
        self.pages
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.state == PageState::Busy)
            .count()
    }

    /// Remove all pages in the `Error` state from the pool.
    /// This frees up capacity so new pages can be registered. You should call this
    /// periodically or after a batch of fetches to reclaim slots.
    pub fn cleanup_error_pages(&self) {
        self.pages
            .lock()
            .unwrap()
            .retain(|p| p.state != PageState::Error);
    }

    /// Take a snapshot of the pool's current statistics.
    /// The snapshot is a point-in-time copy and does not hold the lock after returning.
    pub fn stats(&self) -> PoolStats {
        let pages = self.pages.lock().unwrap();
        PoolStats {
            total_pages: pages.len(),
            busy_pages: pages.iter().filter(|p| p.state == PageState::Busy).count(),
            max_pages: self.max_pages as usize,
        }
    }
}

/// Point-in-time snapshot of page pool utilisation.
///
/// Returned by [`PagePool::stats`]. Use this for monitoring, logging, or deciding
/// whether to wait before starting a new fetch.
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Number of pages currently tracked in the pool (all states).
    pub total_pages: usize,

    /// Number of pages currently in the `Busy` state.
    /// When this equals `max_pages`, no more pages can be created until one finishes.
    pub busy_pages: usize,

    /// Maximum capacity of the pool as configured in [`BrowserConfig::max_pages`].
    pub max_pages: usize,
}
