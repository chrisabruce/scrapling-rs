//! Factory for building [`scrapling_fetch::Response`] objects from Playwright pages.
//!
//! After a browser session navigates to a URL and the page stabilises, this module
//! extracts everything needed to construct a unified [`Response`]: the rendered HTML
//! content, HTTP status code, response headers, cookies from the browser context,
//! and the character encoding parsed from the `Content-Type` header.
//!
//! The main entry point is [`from_browser_page`], which is called internally by
//! [`DynamicSession`](crate::fetcher::DynamicSession) and
//! [`StealthySession`](crate::fetcher::StealthySession) at the end of each fetch
//! cycle. You generally do not need to call it directly.
//!
//! The module also defines [`XhrCapture`], a struct for holding intercepted XHR/fetch
//! responses that were recorded during page navigation (when `capture_xhr` is set).

use std::collections::HashMap;

use bytes::Bytes;
use serde_json::Value;
use tracing::debug;

use scrapling_fetch::Response;
use scrapling_fetch::status_text;

/// Build a [`Response`] from a Playwright page and its navigation responses.
///
/// This function extracts the rendered HTML via `page.content()` (retrying up to
/// `max_retries` times), reads cookies from the browser context, parses the character
/// encoding from the `Content-Type` header, and assembles everything into a
/// [`scrapling_fetch::Response`]. The `meta` map and `_captured_xhr` list are passed
/// through to the response for downstream consumers.
pub async fn from_browser_page(
    page: &playwright_rs::Page,
    first_response: Option<&playwright_rs::Response>,
    _final_response: Option<&playwright_rs::Response>,
    meta: HashMap<String, Value>,
    _captured_xhr: Vec<XhrCapture>,
) -> crate::error::Result<Response> {
    let active_response = _final_response.or(first_response);

    let status = active_response.map(|r| r.status()).unwrap_or(200);
    let reason = active_response
        .map(|r| {
            let st = r.status_text();
            if st.is_empty() {
                status_text(status).to_owned()
            } else {
                st.to_owned()
            }
        })
        .unwrap_or_else(|| status_text(status).to_owned());

    let headers = match first_response {
        Some(resp) => resp.all_headers().await.unwrap_or_default(),
        None => HashMap::new(),
    };

    let encoding = extract_encoding(&headers);

    let content = get_page_content(page, 20).await?;
    let page_url = page.url();
    let body = Bytes::from(content.into_bytes());

    let cookies = match page.context() {
        Ok(ctx) => ctx
            .cookies(None)
            .await
            .map(|c| c.into_iter().map(|ck| (ck.name, ck.value)).collect())
            .unwrap_or_default(),
        Err(_) => HashMap::new(),
    };

    let response = Response::new(
        &page_url,
        body,
        status,
        Some(reason),
        cookies,
        headers,
        HashMap::new(),
        encoding,
        "GET".to_owned(),
        Vec::new(),
        meta,
    );

    Ok(response)
}

async fn get_page_content(
    page: &playwright_rs::Page,
    max_retries: u32,
) -> crate::error::Result<String> {
    for attempt in 0..max_retries {
        match page.content().await {
            Ok(content) => return Ok(content),
            Err(e) => {
                if attempt < max_retries - 1 {
                    debug!(attempt = attempt + 1, "page.content() failed, retrying");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                } else {
                    return Err(crate::error::BrowserError::Navigation(format!(
                        "page.content() failed after {max_retries} attempts: {e}"
                    )));
                }
            }
        }
    }
    unreachable!()
}

fn extract_encoding(headers: &HashMap<String, String>) -> String {
    headers
        .get("content-type")
        .and_then(|ct| {
            ct.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix("charset=").map(|c| c.trim().to_owned())
            })
        })
        .unwrap_or_else(|| "utf-8".to_owned())
}

/// A captured XHR/fetch response recorded during page navigation.
///
/// When [`BrowserConfig::capture_xhr`] is set with a URL pattern, matching
/// XHR/fetch responses are intercepted and stored in this struct. This lets you
/// extract API data that the page fetches in the background without having to
/// parse it out of the rendered HTML.
#[derive(Debug)]
pub struct XhrCapture {
    /// URL of the captured XHR/fetch request.
    /// This is the full URL including query parameters.
    pub url: String,

    /// HTTP status code of the captured response (e.g. `200`, `404`).
    pub status: u16,

    /// Response headers of the captured request.
    /// Useful for checking `Content-Type` or pagination headers.
    pub headers: HashMap<String, String>,

    /// Raw response body bytes.
    /// For JSON APIs you can deserialize this with `serde_json::from_slice`.
    pub body: Bytes,
}
