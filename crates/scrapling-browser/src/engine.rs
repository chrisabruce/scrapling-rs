//! Low-level engine helpers for launching the Playwright driver and building
//! Chromium launch options.
//!
//! This module sits between the high-level session types in [`crate::fetcher`] and the
//! raw Playwright Rust bindings. It has two public functions:
//!
//! - [`build_launch_options`] -- assembles the full set of Chromium CLI flags, proxy
//!   settings, executable path, and channel from a [`BrowserConfig`], merging in
//!   stealth flags when requested and filtering out any harmful automation-revealing
//!   arguments.
//!
//! - [`launch_playwright`] -- starts the Playwright driver process. This must be
//!   called once before any browser can be launched or connected to.
//!
//! You typically do not call these functions directly; [`DynamicSession::start`] and
//! [`StealthySession::start`] use them internally.

use playwright_rs::Playwright;
use playwright_rs::protocol::ProxySettings;
use tracing::info;

use crate::config::BrowserConfig;
use crate::constants::{STEALTH_ARGS, build_args, filter_harmful_args};
use crate::error::Result;

/// Build Playwright launch options from the given browser configuration and stealth flags.
///
/// This function constructs the complete argument list by combining default args,
/// user-supplied `extra_flags`, stealth args (when `stealth` is `true`), and any
/// additional stealth args produced by [`StealthConfig::extra_stealth_args`]. It then
/// strips out harmful flags that would reveal automation (see [`filter_harmful_args`]).
/// Proxy, executable path, channel, and timeout settings from the config are also applied.
pub fn build_launch_options(
    config: &BrowserConfig,
    stealth: bool,
    extra_stealth_args: &[String],
) -> playwright_rs::LaunchOptions {
    let mut args = build_args(stealth);
    args.extend(config.extra_flags.clone());

    if stealth {
        args.extend(STEALTH_ARGS.iter().map(|s| s.to_string()));
    }
    args.extend(extra_stealth_args.iter().cloned());

    if config.dns_over_https {
        args.push("--dns-over-https-templates=https://1.1.1.1/dns-query".into());
    }

    let args = filter_harmful_args(&args);

    let mut opts = playwright_rs::LaunchOptions::new()
        .headless(config.headless)
        .args(args)
        .timeout(config.timeout_ms);

    if let Some(ref path) = config.executable_path {
        opts = opts.executable_path(path.clone());
    }

    if let Some(ref proxy) = config.proxy {
        let ps = ProxySettings {
            server: proxy.server.clone(),
            bypass: None,
            username: proxy.username.clone(),
            password: proxy.password.clone(),
        };
        opts = opts.proxy(ps);
    }

    if config.real_chrome {
        opts = opts.channel("chrome".into());
    }

    opts
}

/// Launch and return a new Playwright driver instance.
///
/// This starts the Playwright Node.js server process that manages browser lifecycle
/// and CDP communication. It must succeed before you can launch or connect to any
/// browser. The returned [`Playwright`] handle should be kept alive for the duration
/// of the session.
pub async fn launch_playwright() -> Result<Playwright> {
    info!("launching Playwright");
    Playwright::launch()
        .await
        .map_err(|e| crate::error::BrowserError::Playwright(e.to_string()))
}
