//! Priority-queue request scheduler with fingerprint-based deduplication.
//!
//! The [`Scheduler`] is the crawl engine's work queue. It accepts [`Request`]
//! values via [`enqueue`](Scheduler::enqueue), computes a SHA-1 fingerprint for
//! each one, rejects duplicates (unless `dont_filter` is set), and hands them
//! back in priority order via [`dequeue`](Scheduler::dequeue).
//!
//! Internally, the scheduler wraps a [`BinaryHeap`] of `PrioritizedRequest`
//! entries. Priority is inverted (stored as negative) so that Rust's max-heap
//! behaves as a min-heap on the negated value, yielding highest-priority-first
//! ordering. A monotonic counter breaks ties in FIFO order.
//!
//! The "seen" set (`HashSet<Vec<u8>>`) stores every fingerprint that has ever
//! been enqueued, preventing the same URL (or URL + method + body combination)
//! from being fetched twice. You can tune what goes into the fingerprint via the
//! constructor flags (`include_kwargs`, `include_headers`, `keep_fragments`).

use std::collections::{BinaryHeap, HashSet};

use crate::request::Request;

/// A priority queue scheduler that deduplicates requests by fingerprint.
///
/// Create one with [`Scheduler::new`], push requests with [`enqueue`](Scheduler::enqueue),
/// and pop them with [`dequeue`](Scheduler::dequeue). The engine creates and
/// owns the scheduler automatically; you only need to interact with it if you
/// are building a custom crawl loop.
pub struct Scheduler {
    queue: BinaryHeap<PrioritizedRequest>,
    seen: HashSet<Vec<u8>>,
    counter: u64,
    include_kwargs: bool,
    include_headers: bool,
    keep_fragments: bool,
}

struct PrioritizedRequest {
    neg_priority: i32,
    counter: u64,
    request: Request,
}

impl PartialEq for PrioritizedRequest {
    fn eq(&self, other: &Self) -> bool {
        self.neg_priority == other.neg_priority && self.counter == other.counter
    }
}

impl Eq for PrioritizedRequest {}

impl PartialOrd for PrioritizedRequest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrioritizedRequest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.neg_priority
            .cmp(&other.neg_priority)
            .then_with(|| self.counter.cmp(&other.counter))
    }
}

impl Scheduler {
    /// Creates a new scheduler with the given fingerprint configuration.
    ///
    /// The three boolean flags control what data is hashed into each request's
    /// fingerprint for deduplication. Setting `include_kwargs` to `true` makes
    /// POST requests with different bodies count as distinct. Setting
    /// `include_headers` to `true` distinguishes requests that differ only in
    /// their HTTP headers. Setting `keep_fragments` to `true` treats
    /// `page#section1` and `page#section2` as different URLs.
    pub fn new(include_kwargs: bool, include_headers: bool, keep_fragments: bool) -> Self {
        Self {
            queue: BinaryHeap::new(),
            seen: HashSet::new(),
            counter: 0,
            include_kwargs,
            include_headers,
            keep_fragments,
        }
    }

    /// Enqueues a request, returning `false` if it was filtered as a duplicate.
    ///
    /// The method first computes (or retrieves) the request's fingerprint, then
    /// checks the "seen" set. If the fingerprint is already known and the
    /// request does not have `dont_filter` set, the request is silently dropped
    /// and the method returns `false`. Otherwise the request is added to the
    /// priority queue and `true` is returned.
    pub fn enqueue(&mut self, mut request: Request) -> bool {
        let fp = request
            .update_fingerprint(
                self.include_kwargs,
                self.include_headers,
                self.keep_fragments,
            )
            .to_vec();

        if !request.dont_filter && self.seen.contains(&fp) {
            return false;
        }

        self.seen.insert(fp);

        let entry = PrioritizedRequest {
            neg_priority: -request.priority,
            counter: self.counter,
            request,
        };
        self.counter += 1;
        self.queue.push(entry);
        true
    }

    /// Removes and returns the highest-priority request, or `None` if the
    /// queue is empty. When two requests share the same priority, the one that
    /// was enqueued first is returned (FIFO tie-breaking).
    pub fn dequeue(&mut self) -> Option<Request> {
        self.queue.pop().map(|entry| entry.request)
    }

    /// Returns the number of pending requests in the queue.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns `true` if the queue has no pending requests.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns a snapshot of all queued requests (sorted by priority then
    /// insertion order) and a reference to the full set of seen fingerprints.
    ///
    /// The checkpoint manager calls this to serialize the crawler's in-progress
    /// state to disk for pause/resume support.
    pub fn snapshot(&self) -> (Vec<&Request>, &HashSet<Vec<u8>>) {
        let mut entries: Vec<_> = self.queue.iter().collect();
        entries.sort_by(|a, b| {
            a.neg_priority
                .cmp(&b.neg_priority)
                .then_with(|| a.counter.cmp(&b.counter))
        });
        let requests: Vec<&Request> = entries.iter().map(|e| &e.request).collect();
        (requests, &self.seen)
    }

    /// Returns the number of unique fingerprints that have been seen so far.
    /// This count only grows -- fingerprints are never removed -- so it
    /// represents the total number of distinct requests encountered during the
    /// crawl, including those that have already been processed.
    pub fn seen_count(&self) -> usize {
        self.seen.len()
    }
}
