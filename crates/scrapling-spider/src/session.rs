//! Named HTTP session management for the crawler.
//!
//! The [`SessionManager`] lets a spider register multiple HTTP backends under
//! string identifiers. Each [`Request`](crate::request::Request) carries an
//! optional `sid` (session ID) that tells the engine which backend to use for
//! that particular fetch. This is how you mix plain HTTP fetchers with
//! cookie-persisting sessions, or route certain domains through different proxy
//! configurations.
//!
//! A session is either a stateless [`Fetcher`] or a stateful [`FetcherSession`]
//! (which automatically carries cookies and custom headers across requests).
//! The spider configures its sessions in
//! [`Spider::configure_sessions`](crate::spider::Spider::configure_sessions).
//!
//! ```rust,ignore
//! use scrapling_spider::session::{Session, SessionManager};
//! use scrapling_fetch::Fetcher;
//!
//! let mut manager = SessionManager::new();
//! manager.add("default", Session::Fetcher(Fetcher::new()), true).unwrap();
//! ```

use std::collections::HashMap;

use scrapling_fetch::{Fetcher, FetcherSession, Response};
use tracing::debug;

use crate::error::{Result, SpiderError};
use crate::request::Request;

/// A session backend that can be either a stateless fetcher or a stateful session.
///
/// The distinction matters for cookie handling: a [`Fetcher`] starts fresh on
/// every request, while a [`FetcherSession`] preserves cookies, headers, and
/// other state between requests. Choose the variant that matches your crawling
/// needs.
pub enum Session {
    /// A stateless HTTP fetcher. Each request is independent -- no cookies or
    /// headers carry over. This is the simplest option and works well for public
    /// pages that do not require authentication.
    Fetcher(Fetcher),
    /// A stateful HTTP session that persists cookies, headers, and other
    /// connection-level state across requests. Use this when you need to log in,
    /// maintain a CSRF token, or interact with session-dependent APIs.
    FetcherSession(FetcherSession),
}

/// Manages named sessions and dispatches fetch requests to the appropriate one.
///
/// The engine owns a single `SessionManager` and uses it for every fetch during
/// the crawl. Sessions are identified by string IDs; one session is designated
/// as the default and is used whenever a [`Request`](crate::request::Request)
/// does not specify a `sid`.
pub struct SessionManager {
    sessions: HashMap<String, Session>,
    default_session_id: Option<String>,
}

impl SessionManager {
    /// Creates an empty session manager with no sessions registered. You must
    /// call [`add`](SessionManager::add) at least once before the engine can
    /// fetch anything; the engine will return an error if the manager is empty.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            default_session_id: None,
        }
    }

    /// Registers a session under the given ID, optionally marking it as the
    /// default. The first session added automatically becomes the default even
    /// if `default` is `false`. Returns an error if a session with the same ID
    /// already exists.
    pub fn add(
        &mut self,
        session_id: impl Into<String>,
        session: Session,
        default: bool,
    ) -> Result<()> {
        let id = session_id.into();
        if self.sessions.contains_key(&id) {
            return Err(SpiderError::Session(format!(
                "session '{id}' already exists"
            )));
        }
        if default || self.default_session_id.is_none() {
            self.default_session_id = Some(id.clone());
        }
        self.sessions.insert(id, session);
        Ok(())
    }

    /// Returns the default session ID, or an error if none is set. The engine
    /// calls this whenever a request has an empty `sid` field to determine which
    /// session to use.
    pub fn default_session_id(&self) -> Result<&str> {
        self.default_session_id
            .as_deref()
            .ok_or_else(|| SpiderError::Session("no default session".into()))
    }

    /// Returns a list of all registered session IDs.
    pub fn session_ids(&self) -> Vec<&str> {
        self.sessions.keys().map(|s| s.as_str()).collect()
    }

    /// Returns a reference to the session with the given ID, or an error if not found.
    pub fn get(&self, session_id: &str) -> Result<&Session> {
        self.sessions.get(session_id).ok_or_else(|| {
            SpiderError::Session(format!(
                "session '{session_id}' not found; available: {:?}",
                self.session_ids()
            ))
        })
    }

    /// Returns a mutable reference to the session with the given ID, or an error if not found.
    pub fn get_mut(&mut self, session_id: &str) -> Result<&mut Session> {
        let ids = self.session_ids().join(", ");
        self.sessions.get_mut(session_id).ok_or_else(|| {
            SpiderError::Session(format!(
                "session '{session_id}' not found; available: [{ids}]"
            ))
        })
    }

    /// Fetches the URL from the request using the appropriate session.
    ///
    /// If the request has a non-empty `sid`, that session is used; otherwise
    /// the default session is selected. The method dispatches to the correct
    /// `get` implementation depending on whether the session is a [`Fetcher`]
    /// or a [`FetcherSession`]. Returns the HTTP response on success, or a
    /// [`SpiderError`](crate::error::SpiderError) on failure.
    pub async fn fetch(&self, request: &Request) -> Result<Response> {
        let sid = if request.sid.is_empty() {
            self.default_session_id()?
        } else {
            &request.sid
        };

        let session = self.get(sid)?;

        debug!(sid = sid, url = %request.url, "fetching via session");

        let response = match session {
            Session::Fetcher(f) => f.get(&request.url, None).await?,
            Session::FetcherSession(fs) => fs.get(&request.url, None).await?,
        };

        Ok(response)
    }

    /// Returns the number of registered sessions.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Returns `true` if no sessions are registered.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Returns `true` if a session with the given ID exists.
    pub fn contains(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
