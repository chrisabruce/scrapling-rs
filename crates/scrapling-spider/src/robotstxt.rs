//! Robots.txt fetching, parsing, and enforcement.
//!
//! When [`Spider::robots_txt_obey`](crate::spider::Spider::robots_txt_obey)
//! returns `true`, the engine creates a [`RobotsTxtManager`] and consults it
//! before every fetch. The manager lazily fetches and parses each domain's
//! `robots.txt` the first time it is needed, then caches the result for the
//! remainder of the crawl.
//!
//! Only rules under the `User-agent: *` section are respected. The parser
//! extracts `Disallow` directives (matched as path prefixes) and an optional
//! `Crawl-delay` value. Domains whose `robots.txt` cannot be fetched or parsed
//! are treated as "allow all."

use std::collections::HashMap;

use crate::session::SessionManager;

/// Fetches, parses, and caches robots.txt rules per domain.
///
/// You do not construct this directly; the [`CrawlerEngine`](crate::spider::CrawlerEngine)
/// creates one when `robots_txt_obey` is enabled. The manager maintains an
/// internal cache so that each domain's `robots.txt` is fetched at most once
/// per crawl run.
pub struct RobotsTxtManager {
    cache: HashMap<String, RobotsTxtRules>,
}

struct RobotsTxtRules {
    disallowed: Vec<String>,
    crawl_delay: Option<f64>,
}

impl Default for RobotsTxtManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RobotsTxtManager {
    /// Creates a new robots.txt manager with an empty cache. Rules will be
    /// fetched on demand the first time a URL on a new domain is checked.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Returns `true` if the URL is allowed by the domain's robots.txt rules.
    ///
    /// The method extracts the domain from the URL, fetches and parses
    /// `robots.txt` if it hasn't been cached yet, and checks whether the URL's
    /// path matches any `Disallow` directive. URLs that cannot be parsed (e.g.,
    /// invalid format) are considered allowed.
    pub async fn can_fetch(
        &mut self,
        url: &str,
        sid: &str,
        session_manager: &SessionManager,
    ) -> bool {
        let Some(domain) = extract_domain(url) else {
            return true;
        };

        let rules = self.get_or_fetch(&domain, sid, session_manager).await;

        let path = url::Url::parse(url)
            .ok()
            .map(|u| u.path().to_owned())
            .unwrap_or_else(|| "/".into());

        !rules.disallowed.iter().any(|d| path.starts_with(d))
    }

    /// Returns the crawl-delay specified in the domain's robots.txt, if any.
    /// The delay is in seconds and comes from the `Crawl-delay` directive under
    /// `User-agent: *`. Returns `None` if no delay is specified or if the
    /// domain's `robots.txt` could not be fetched.
    pub async fn get_crawl_delay(
        &mut self,
        url: &str,
        sid: &str,
        session_manager: &SessionManager,
    ) -> Option<f64> {
        let domain = extract_domain(url)?;
        let rules = self.get_or_fetch(&domain, sid, session_manager).await;
        rules.crawl_delay
    }

    /// Pre-fetches robots.txt for all unique domains in the given URLs.
    ///
    /// The engine calls this before the crawl loop starts so that the first
    /// batch of requests is not delayed by on-demand robots.txt lookups.
    /// Duplicate domains are deduplicated internally.
    pub async fn prefetch(&mut self, urls: &[String], sid: &str, session_manager: &SessionManager) {
        let mut domains_seen = std::collections::HashSet::new();
        for url in urls {
            if let Some(domain) = extract_domain(url) {
                if domains_seen.insert(domain.clone()) {
                    self.get_or_fetch(&domain, sid, session_manager).await;
                }
            }
        }
    }

    async fn get_or_fetch(
        &mut self,
        domain: &str,
        sid: &str,
        session_manager: &SessionManager,
    ) -> &RobotsTxtRules {
        if !self.cache.contains_key(domain) {
            let rules = fetch_and_parse(domain, sid, session_manager).await;
            self.cache.insert(domain.to_owned(), rules);
        }
        self.cache.get(domain).unwrap()
    }
}

async fn fetch_and_parse(
    domain: &str,
    sid: &str,
    session_manager: &SessionManager,
) -> RobotsTxtRules {
    let robots_url = format!("https://{domain}/robots.txt");

    let content = match session_manager.get(if sid.is_empty() {
        session_manager.default_session_id().unwrap_or("default")
    } else {
        sid
    }) {
        Ok(_session) => {
            let req = crate::request::Request::new(&robots_url);
            match session_manager.fetch(&req).await {
                Ok(resp) if resp.is_success() => String::from_utf8_lossy(&resp.body).to_string(),
                _ => String::new(),
            }
        }
        Err(_) => String::new(),
    };

    parse_robots_txt(&content)
}

fn parse_robots_txt(content: &str) -> RobotsTxtRules {
    let mut disallowed = Vec::new();
    let mut crawl_delay = None;
    let mut in_wildcard_agent = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        let value = value.trim();

        match key.as_str() {
            "user-agent" => {
                in_wildcard_agent = value == "*";
            }
            "disallow" if in_wildcard_agent && !value.is_empty() => {
                disallowed.push(value.to_owned());
            }
            "crawl-delay" if in_wildcard_agent => {
                if let Ok(delay) = value.parse::<f64>() {
                    crawl_delay = Some(delay);
                }
            }
            _ => {}
        }
    }

    RobotsTxtRules {
        disallowed,
        crawl_delay,
    }
}

fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_robots_txt_basic() {
        let rules = parse_robots_txt(
            "User-agent: *\nDisallow: /admin\nDisallow: /private\nCrawl-delay: 2\n",
        );
        assert_eq!(rules.disallowed, vec!["/admin", "/private"]);
        assert_eq!(rules.crawl_delay, Some(2.0));
    }

    #[test]
    fn parse_robots_txt_empty() {
        let rules = parse_robots_txt("");
        assert!(rules.disallowed.is_empty());
        assert!(rules.crawl_delay.is_none());
    }

    #[test]
    fn parse_robots_txt_specific_agent_ignored() {
        let rules = parse_robots_txt("User-agent: Googlebot\nDisallow: /secret\n");
        assert!(rules.disallowed.is_empty());
    }

    #[test]
    fn extract_domain_works() {
        assert_eq!(
            extract_domain("https://example.com/page"),
            Some("example.com".into())
        );
        assert_eq!(extract_domain("not-a-url"), None);
    }
}
