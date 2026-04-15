//! Chromium launch arguments and resource-type constants.
//!
//! This module defines the static data that controls how the Chromium browser process
//! is configured at launch time. There are four categories:
//!
//! - [`EXTRA_RESOURCES`] -- the resource types (e.g. `"font"`, `"image"`) that are
//!   blocked when [`BrowserConfig::disable_resources`] is `true`.
//!
//! - [`DEFAULT_ARGS`] -- baseline Chromium CLI flags applied to every launch. These
//!   disable unnecessary features and improve startup speed.
//!
//! - [`STEALTH_ARGS`] -- a large set of flags that remove automation indicators and
//!   reduce the browser's fingerprint surface. Only applied in stealth mode.
//!
//! - [`HARMFUL_ARGS`] -- flags that *reveal* the browser is automated. These are
//!   stripped from the final argument list by [`filter_harmful_args`], even if the
//!   user accidentally includes them in `extra_flags`.
//!
//! The two helper functions [`build_args`] and [`filter_harmful_args`] compose and
//! sanitise the argument list for use in [`engine::build_launch_options`].

/// Resource types to block when `disable_resources` is enabled.
/// These are Playwright resource type strings matched against each outgoing request
/// in [`intercept::should_block_resource`].
pub const EXTRA_RESOURCES: &[&str] = &[
    "font",
    "image",
    "media",
    "beacon",
    "object",
    "imageset",
    "texttrack",
    "websocket",
    "csp_report",
    "stylesheet",
];

/// Chromium flags that reveal automation and should be stripped from launch args.
///
/// Bot-detection systems look for these flags to determine whether a browser was
/// launched programmatically. The [`filter_harmful_args`] function removes any
/// argument that starts with one of these prefixes.
pub const HARMFUL_ARGS: &[&str] = &[
    "--enable-automation",
    "--disable-popup-blocking",
    "--disable-component-update",
    "--disable-default-apps",
    "--disable-extensions",
];

/// Default Chromium launch flags for speed and basic stealth.
///
/// These are applied to every browser launch (both standard and stealth modes).
/// They disable crash reporting, info bars, first-run wizards, and other features
/// that are irrelevant in an automation context and can slow down startup.
pub const DEFAULT_ARGS: &[&str] = &[
    "--no-pings",
    "--no-first-run",
    "--disable-infobars",
    "--disable-breakpad",
    "--no-service-autorun",
    "--homepage=about:blank",
    "--password-store=basic",
    "--disable-hang-monitor",
    "--no-default-browser-check",
    "--disable-session-crashed-bubble",
    "--disable-search-engine-choice-screen",
];

/// Anti-detection Chromium flags for evading bot detection systems.
///
/// This is a comprehensive list of flags that hide automation indicators, disable
/// features that leak information (e.g. crash reporters, sync, translate), and
/// configure Blink to report plausible device capabilities. Only applied when the
/// session is in stealth mode.
pub const STEALTH_ARGS: &[&str] = &[
    "--test-type",
    "--lang=en-US",
    "--mute-audio",
    "--disable-sync",
    "--hide-scrollbars",
    "--disable-logging",
    "--start-maximized",
    "--enable-async-dns",
    "--accept-lang=en-US",
    "--use-mock-keychain",
    "--disable-translate",
    "--disable-voice-input",
    "--window-position=0,0",
    "--disable-wake-on-wifi",
    "--ignore-gpu-blocklist",
    "--enable-tcp-fast-open",
    "--enable-web-bluetooth",
    "--disable-cloud-import",
    "--disable-print-preview",
    "--disable-dev-shm-usage",
    "--metrics-recording-only",
    "--disable-crash-reporter",
    "--disable-partial-raster",
    "--disable-gesture-typing",
    "--disable-checker-imaging",
    "--disable-prompt-on-repost",
    "--force-color-profile=srgb",
    "--font-render-hinting=none",
    "--aggressive-cache-discard",
    "--disable-cookie-encryption",
    "--disable-domain-reliability",
    "--disable-threaded-animation",
    "--disable-threaded-scrolling",
    "--enable-simple-cache-backend",
    "--disable-background-networking",
    "--enable-surface-synchronization",
    "--disable-image-animation-resync",
    "--disable-renderer-backgrounding",
    "--disable-ipc-flooding-protection",
    "--prerender-from-omnibox=disabled",
    "--safebrowsing-disable-auto-update",
    "--disable-offer-upload-credit-cards",
    "--disable-background-timer-throttling",
    "--disable-new-content-rendering-timeout",
    "--run-all-compositor-stages-before-draw",
    "--disable-client-side-phishing-detection",
    "--disable-backgrounding-occluded-windows",
    "--disable-layer-tree-host-memory-pressure",
    "--autoplay-policy=user-gesture-required",
    "--disable-offer-store-unmasked-wallet-cards",
    "--disable-blink-features=AutomationControlled",
    "--disable-component-extensions-with-background-pages",
    "--enable-features=NetworkService,NetworkServiceInProcess,TrustTokens,TrustTokensAlwaysAllowIssuance",
    "--blink-settings=primaryHoverType=2,availableHoverTypes=2,primaryPointerType=4,availablePointerTypes=4",
    "--disable-features=AudioServiceOutOfProcess,TranslateUI,BlinkGenPropertyTrees",
];

/// Builds the Chromium launch argument list, optionally including stealth flags.
///
/// When `stealth` is `true`, the [`STEALTH_ARGS`] are appended after the
/// [`DEFAULT_ARGS`]. The result should be further processed by
/// [`filter_harmful_args`] before being passed to Playwright.
pub fn build_args(stealth: bool) -> Vec<String> {
    let mut args: Vec<String> = DEFAULT_ARGS.iter().map(|s| s.to_string()).collect();
    if stealth {
        args.extend(STEALTH_ARGS.iter().map(|s| s.to_string()));
    }
    args
}

/// Removes automation-revealing flags from the argument list.
///
/// Iterates over the provided args and drops any that start with a prefix in
/// [`HARMFUL_ARGS`]. This is a safety net -- even if a user adds `--enable-automation`
/// in `extra_flags`, it will be silently removed here.
pub fn filter_harmful_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|a| !HARMFUL_ARGS.iter().any(|h| a.starts_with(h)))
        .cloned()
        .collect()
}
