# Spider Architecture

This document explains how scrapling-rs spiders work internally: the components involved, how data flows between them, and what happens during each iteration of the crawl loop.

## Architecture Diagram

```
                          +------------------+
                          |     Spider       |
                          |  (your code)     |
                          +--------+---------+
                                   |
                    start_urls() / parse()
                                   |
                                   v
+--------+   enqueue   +----------+-----------+   dequeue   +---------+
| Spider |------------>|      Scheduler       |------------>| Engine  |
| Output |             | (priority queue +    |             | (crawl  |
|        |             |  dedup fingerprints) |             |  loop)  |
+--------+             +----------------------+             +----+----+
     ^                                                           |
     |                                                           |
     |   items / follow requests                        fetch    |
     |                                                           v
     |                                                 +---------+---------+
     +---------- parse(response) <---------------------| SessionManager    |
                                                       | +-- "default" --> Fetcher
                                                       | +-- "auth"    --> FetcherSession
                                                       | +-- "browser" --> (browser session)
                                                       +-------------------+
                                                                |
                                                                v
                                                         +------+------+
                                                         |  Response   |
                                                         | (lazy HTML  |
                                                         |  parsing)   |
                                                         +-------------+

Side channels (optional):
  +------------------+    +---------------------+    +-------------------+
  | RobotsTxtManager |    | ResponseCacheManager|    | CheckpointManager |
  | (per-domain      |    | (dev mode only,     |    | (pause/resume,    |
  |  disallow rules) |    |  disk-backed)       |    |  JSON snapshots)  |
  +------------------+    +---------------------+    +-------------------+
```

## Components

### Spider (trait)

The `Spider` trait is the interface you implement. It defines:

- **What to crawl** -- `start_urls()` and `start_requests()` produce the initial work.
- **How to parse** -- `parse()` receives each `Response` and returns `Vec<SpiderOutput>`, which contains scraped items and/or follow-up requests.
- **Configuration knobs** -- `concurrent_requests()`, `download_delay()`, `allowed_domains()`, `robots_txt_obey()`, `development_mode()`, and fingerprint settings.
- **Lifecycle hooks** -- `on_start()`, `on_close()`, `on_error()`, `on_scraped_item()`, `is_blocked()`.
- **Session setup** -- `configure_sessions()` registers fetcher backends with the session manager.

The trait has sensible defaults for everything except `name()`, `start_urls()`, and `parse()`.

### CrawlerEngine

The engine is the runtime. It owns all infrastructure and runs the main loop:

- Holds a reference to your `Spider` (as `&dyn Spider`).
- Creates and manages the `Scheduler`, `SessionManager`, and optional managers for robots.txt, response caching, and checkpointing.
- Enforces concurrency limits and download delays.
- Routes responses to the correct callback (per-request or the spider's `parse`).
- Collects scraped items into an `ItemList` and aggregates statistics in `CrawlStats`.

### Scheduler

A priority queue with fingerprint-based deduplication:

- Requests are ordered by priority (higher values dequeued first), with FIFO tie-breaking.
- Each request gets a SHA-1 fingerprint computed from its URL, session ID, HTTP method, and optionally its body, headers, and URL fragment.
- Duplicate fingerprints are silently dropped unless the request has `dont_filter: true`.
- Supports snapshotting for checkpoint serialization.

### SessionManager

A registry of named HTTP backends:

- Each session has a string ID (e.g., `"default"`, `"auth"`, `"browser"`).
- Sessions are either a stateless `Fetcher` (no cookies) or a stateful `FetcherSession` (persistent cookies and headers).
- One session is marked as the default and is used when a request does not specify a `sid`.
- The `fetch()` method dispatches to the correct session based on the request's `sid` field.

### Request

The unit of work in the pipeline. Carries:

- A URL to fetch.
- A session ID for routing.
- A priority for scheduling.
- Arbitrary metadata (the `meta` map) for passing context between parse stages.
- An optional per-request callback that overrides the spider's `parse` method.
- Retry state and deduplication fingerprint.

### SpiderOutput

The return type of `parse()`. Each value is either:

- `SpiderOutput::Item(serde_json::Value)` -- a scraped data record, collected into `ItemList`.
- `SpiderOutput::FollowRequest(Request)` -- a new URL to crawl, fed back into the `Scheduler`.

### Response

The HTTP response from a fetch, provided by `scrapling_fetch`. Key features:

- Lazy HTML parsing -- the DOM is only built when you call `css()`, `selector()`, or `text()`.
- Convenience methods for URL resolution (`urljoin`, `follow_url`), content conversion (`to_markdown`, `to_text`), and status checking (`is_success`, `is_blocked`).
- Carries cookies, headers, encoding, and redirect history.

## The Crawl Loop, Step by Step

Here is exactly what happens inside `CrawlerEngine::crawl()`:

### 1. Restore or Initialize

The engine checks for an existing checkpoint file on disk. If one is found, it restores the pending request URLs and seen fingerprint set from the snapshot and sets `resuming = true`. If no checkpoint exists, the spider's `start_requests()` are enqueued into the scheduler.

### 2. Call on_start

The spider's `on_start(resuming)` hook fires. Use this for one-time setup like opening database connections or logging the crawl start.

### 3. Prefetch robots.txt

If `robots_txt_obey()` returns `true`, the engine fetches `robots.txt` for every domain in the start URLs before beginning the main loop. This prevents the first batch of requests from blocking on robots.txt lookups.

### 4. Main Loop

The engine enters a loop that continues until the scheduler is empty and no tasks are in flight (or until a pause/force-stop signal arrives):

```
while scheduler has work OR tasks are in flight:
    if pause requested:
        wait for in-flight tasks, save checkpoint, break

    if periodic checkpoint is due:
        save checkpoint

    if scheduler is empty:
        wait briefly, continue

    if at concurrency limit:
        wait briefly, continue

    dequeue next request
    apply download delay (if configured)
    process the request
```

### 5. Process Each Request

For each dequeued request, the engine runs this pipeline:

1. **Robots.txt check** -- If enabled, verify the URL is allowed. If disallowed, increment `robots_disallowed_count` and skip.

2. **Cache check** -- If development mode is on, look up the request's fingerprint in the response cache. On a cache hit, skip the network fetch and use the cached response.

3. **Fetch** -- Resolve the session ID (falling back to default if empty) and call `SessionManager::fetch()`. On failure, increment `failed_requests_count`, call `on_error()`, and skip.

4. **Record statistics** -- Increment status code counters, byte counters (global and per-domain), and per-session request counts.

5. **Cache the response** -- If development mode is on, store the response for future runs.

6. **Blocked check** -- Call `is_blocked()` on the response. If blocked and under the retry limit, re-enqueue with lower priority and `dont_filter: true`. If over the limit, log a warning and drop the request.

7. **Run callbacks** -- If the request has a per-request callback, call it. Otherwise, call the spider's `parse()` method. The callback returns `Vec<SpiderOutput>`.

8. **Dispatch outputs** -- For each output:
   - `Item`: pass through `on_scraped_item()`. If the hook returns `Some`, add to `ItemList` and increment `items_scraped`. If `None`, increment `items_dropped`. If streaming is enabled, also send to the channel.
   - `FollowRequest`: check domain restrictions, then enqueue into the scheduler.

### 6. Cleanup

After the loop exits:

- The spider's `on_close()` hook fires.
- If the crawl completed normally (not paused), the checkpoint file is deleted.
- Final timestamps are recorded in `CrawlStats`.
- The stats are returned to the caller.

## Data Flow Summary

```
start_urls() --> Request --> Scheduler --> Engine --> SessionManager --> HTTP
                                                                         |
                                                                    Response
                                                                         |
                                              parse() or callback <------+
                                                     |
                                        Vec<SpiderOutput>
                                           /              \
                                Item(json)            FollowRequest(req)
                                    |                       |
                             on_scraped_item()         Scheduler.enqueue()
                                    |
                               ItemList
                                    |
                          to_json() / to_jsonl()
```

## Next Steps

- [Getting Started](getting-started.md) -- write your first spider
- [Session Management](sessions.md) -- configure multiple HTTP backends
- [Requests and Responses](requests-responses.md) -- the Request builder and Response API
- [Advanced Features](advanced.md) -- concurrency, checkpointing, streaming, and more
