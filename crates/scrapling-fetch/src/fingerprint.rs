//! Browser fingerprint generation for stealth HTTP requests.
//!
//! Bot-detection systems inspect HTTP headers like `User-Agent`, `Sec-Ch-Ua`, and
//! `Sec-Fetch-*` to distinguish real browsers from automated tools. This module
//! generates realistic header sets that match what Chrome, Firefox, and Edge actually
//! send, including platform-specific details derived from the OS this code was compiled on.
//!
//! The key entry points are:
//!
//! - [`generate_headers`] -- produces a full set of browser-like headers (User-Agent,
//!   Accept, Sec-Ch-Ua, Sec-Fetch-*, etc.) for a randomly or explicitly chosen browser.
//! - [`default_user_agent`] -- returns a Chrome User-Agent string for the current OS.
//!   Used as a fallback when no impersonation or stealth headers are configured.
//!
//! Browser version constants (`CHROME_VERSION`, `FIREFOX_VERSION`, `EDGE_VERSION`) are
//! defined at the top of this file and should be updated periodically to stay current.

use std::collections::HashMap;

/// Supported operating system targets for fingerprint generation.
///
/// The OS determines the platform token inside User-Agent strings (e.g., `"Windows NT
/// 10.0; Win64; x64"` vs `"Macintosh; Intel Mac OS X 10_15_7"`). This is detected at
/// compile time via [`detect_os`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsName {
    /// Linux-based operating system.
    Linux,
    /// macOS operating system.
    MacOs,
    /// Windows operating system.
    Windows,
}

/// Detects the current operating system at compile time using `cfg!(target_os)`.
/// Returns [`OsName::Linux`] as a fallback for any platform that is not macOS or Windows.
pub fn detect_os() -> OsName {
    if cfg!(target_os = "macos") {
        OsName::MacOs
    } else if cfg!(target_os = "windows") {
        OsName::Windows
    } else {
        OsName::Linux
    }
}

const CHROME_VERSION: u32 = 145;
const FIREFOX_VERSION: u32 = 142;
const EDGE_VERSION: u32 = 140;

fn platform_string(os: OsName, include_rv: bool) -> &'static str {
    match (os, include_rv) {
        (OsName::Windows, false) => "Windows NT 10.0; Win64; x64",
        (OsName::MacOs, false) => "Macintosh; Intel Mac OS X 10_15_7",
        (OsName::Linux, false) => "X11; Linux x86_64",
        (OsName::Windows, true) => "Windows NT 10.0; Win64; x64; rv:142.0",
        (OsName::MacOs, true) => "Macintosh; Intel Mac OS X 10.15; rv:142.0",
        (OsName::Linux, true) => "X11; Linux x86_64; rv:142.0",
    }
}

fn sec_ch_platform(os: OsName) -> &'static str {
    match os {
        OsName::Windows => "\"Windows\"",
        OsName::MacOs => "\"macOS\"",
        OsName::Linux => "\"Linux\"",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserKind {
    Chrome,
    Firefox,
    Edge,
}

impl BrowserKind {
    fn user_agent(self, os: OsName) -> String {
        match self {
            Self::Chrome => format!(
                "Mozilla/5.0 ({}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{CHROME_VERSION}.0.0.0 Safari/537.36",
                platform_string(os, false)
            ),
            Self::Firefox => format!(
                "Mozilla/5.0 ({}) Gecko/20100101 Firefox/{FIREFOX_VERSION}.0",
                platform_string(os, true)
            ),
            Self::Edge => format!(
                "Mozilla/5.0 ({}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{CHROME_VERSION}.0.0.0 Safari/537.36 Edg/{EDGE_VERSION}.0.0.0",
                platform_string(os, false)
            ),
        }
    }

    fn sec_ch_ua(self) -> String {
        match self {
            Self::Edge => format!(
                "\"Microsoft Edge\";v=\"{EDGE_VERSION}\", \"Chromium\";v=\"{CHROME_VERSION}\", \"Not-A.Brand\";v=\"99\""
            ),
            Self::Chrome => format!(
                "\"Google Chrome\";v=\"{CHROME_VERSION}\", \"Chromium\";v=\"{CHROME_VERSION}\", \"Not-A.Brand\";v=\"99\""
            ),
            Self::Firefox => String::new(),
        }
    }

    fn random() -> Self {
        use rand::Rng;
        const CHOICES: [BrowserKind; 3] =
            [BrowserKind::Chrome, BrowserKind::Firefox, BrowserKind::Edge];
        CHOICES[rand::thread_rng().gen_range(0..CHOICES.len())]
    }
}

/// Returns a Chrome user-agent string for the given OS. This includes the full
/// `Mozilla/5.0 (...) AppleWebKit/537.36 ... Chrome/VERSION ...` format that real
/// Chrome browsers send.
pub fn chrome_user_agent(os: OsName) -> String {
    BrowserKind::Chrome.user_agent(os)
}

/// Generates a full set of realistic browser headers for bypass of bot detection.
///
/// When `browser_mode` is `true`, the headers are always Chrome-based (used when wreq
/// browser impersonation is active, since the TLS fingerprint is already Chrome).
/// When `false`, a browser is randomly chosen from Chrome, Firefox, and Edge to add
/// diversity across requests. Chromium-based browsers include `Sec-Ch-Ua` and
/// `Sec-Fetch-*` headers; Firefox does not.
pub fn generate_headers(browser_mode: bool) -> HashMap<String, String> {
    let os = detect_os();
    let browser = if browser_mode {
        BrowserKind::Chrome
    } else {
        BrowserKind::random()
    };

    let mut headers = HashMap::from([
        ("User-Agent".into(), browser.user_agent(os)),
        ("Accept".into(), "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8".into()),
        ("Accept-Language".into(), "en-US,en;q=0.9".into()),
        ("Accept-Encoding".into(), "gzip, deflate, br".into()),
        ("Upgrade-Insecure-Requests".into(), "1".into()),
    ]);

    if matches!(browser, BrowserKind::Chrome | BrowserKind::Edge) {
        headers.insert("Sec-Ch-Ua-Platform".into(), sec_ch_platform(os).into());
        headers.insert("Sec-Fetch-Site".into(), "none".into());
        headers.insert("Sec-Fetch-Mode".into(), "navigate".into());
        headers.insert("Sec-Fetch-User".into(), "?1".into());
        headers.insert("Sec-Fetch-Dest".into(), "document".into());
        headers.insert("Sec-Ch-Ua".into(), browser.sec_ch_ua());
        headers.insert("Sec-Ch-Ua-Mobile".into(), "?0".into());
    }

    headers
}

/// Returns the default user-agent string (Chrome on the detected OS). This is used
/// as a last-resort fallback when neither stealth headers nor browser impersonation
/// are enabled and no custom User-Agent header has been set.
pub fn default_user_agent() -> String {
    chrome_user_agent(detect_os())
}
