//! Request interception logic for blocking unwanted network requests.
//!
//! When a browser page has resource blocking or domain blocking enabled, every
//! outgoing request is evaluated by the functions in this module before it is
//! allowed to proceed. There are two independent blocking mechanisms:
//!
//! 1. **Resource-type blocking** ([`should_block_resource`]) -- drops requests for
//!    heavyweight resource types like images, fonts, stylesheets, and media when
//!    `disable_resources` is `true`. The exact list of blocked types is defined in
//!    [`constants::EXTRA_RESOURCES`].
//!
//! 2. **Domain blocking** ([`is_domain_blocked`]) -- drops requests whose hostname
//!    (or any parent domain suffix) appears in a blocklist. This powers both custom
//!    domain blocking and the built-in ad/tracker blocklist.
//!
//! The top-level [`should_block_request`] function combines both checks and is the
//! one wired into Playwright's route handler in [`crate::fetcher`].

use std::collections::HashSet;

use crate::constants::EXTRA_RESOURCES;

/// Returns `true` if the given resource type should be blocked when resource blocking is enabled.
///
/// The check is skipped entirely when `disable_resources` is `false`. When enabled,
/// it matches `resource_type` against the list in [`constants::EXTRA_RESOURCES`],
/// which includes types like `"font"`, `"image"`, `"stylesheet"`, and `"media"`.
pub fn should_block_resource(resource_type: &str, disable_resources: bool) -> bool {
    if !disable_resources {
        return false;
    }
    EXTRA_RESOURCES.contains(&resource_type)
}

/// Returns `true` if `hostname` (or any of its parent domains) appears in `blocked_domains`.
///
/// The check walks up the domain suffix chain, so blocking `"doubleclick.net"` will
/// also block `"ad.doubleclick.net"` and `"sub.ad.doubleclick.net"`. Returns `false`
/// immediately when `blocked_domains` is empty, avoiding unnecessary work.
pub fn is_domain_blocked(hostname: &str, blocked_domains: &HashSet<String>) -> bool {
    if blocked_domains.is_empty() {
        return false;
    }

    if blocked_domains.contains(hostname) {
        return true;
    }

    // Walk up the domain suffix chain: "sub.ads.example.com" checks
    // "ads.example.com", "example.com", "com"
    let mut parts: &str = hostname;
    while let Some(pos) = parts.find('.') {
        parts = &parts[pos + 1..];
        if blocked_domains.contains(parts) {
            return true;
        }
    }

    false
}

/// Returns `true` if a request should be blocked based on its resource type or domain.
///
/// This is the top-level function called from Playwright's route handler. It first
/// checks resource-type blocking, then parses the URL and checks domain blocking.
/// If either check matches, the request is blocked.
pub fn should_block_request(
    resource_type: &str,
    url: &str,
    disable_resources: bool,
    blocked_domains: &HashSet<String>,
) -> bool {
    if should_block_resource(resource_type, disable_resources) {
        return true;
    }

    if !blocked_domains.is_empty() {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                return is_domain_blocked(&host.to_lowercase(), blocked_domains);
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_blocking() {
        assert!(should_block_resource("font", true));
        assert!(should_block_resource("image", true));
        assert!(should_block_resource("stylesheet", true));
        assert!(!should_block_resource("document", true));
        assert!(!should_block_resource("font", false));
    }

    #[test]
    fn domain_blocking_exact() {
        let mut domains = HashSet::new();
        domains.insert("ads.example.com".to_owned());
        assert!(is_domain_blocked("ads.example.com", &domains));
        assert!(!is_domain_blocked("example.com", &domains));
    }

    #[test]
    fn domain_blocking_suffix() {
        let mut domains = HashSet::new();
        domains.insert("doubleclick.net".to_owned());
        assert!(is_domain_blocked("ad.doubleclick.net", &domains));
        assert!(is_domain_blocked("sub.ad.doubleclick.net", &domains));
        assert!(is_domain_blocked("doubleclick.net", &domains));
        assert!(!is_domain_blocked("notdoubleclick.net", &domains));
    }

    #[test]
    fn domain_blocking_empty() {
        let domains = HashSet::new();
        assert!(!is_domain_blocked("anything.com", &domains));
    }

    #[test]
    fn should_block_request_combined() {
        let mut domains = HashSet::new();
        domains.insert("tracker.com".to_owned());

        assert!(should_block_request(
            "document",
            "https://tracker.com/pixel",
            false,
            &domains
        ));
        assert!(should_block_request(
            "font",
            "https://cdn.com/font.woff",
            true,
            &domains
        ));
        assert!(!should_block_request(
            "document",
            "https://example.com",
            false,
            &domains
        ));
    }
}
