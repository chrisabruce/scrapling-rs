//! HTTP status code to reason phrase mapping.
//!
//! This module provides a single function, [`status_text`], that maps numeric HTTP
//! status codes to their standard reason phrases (e.g., 200 -> "OK", 404 -> "Not Found").
//! It is used by [`Response::new`](crate::Response::new) to populate the `reason` field
//! when the upstream HTTP client does not provide one.
//!
//! The lookup table covers all standard status codes defined in RFCs 7231, 7538, 6585,
//! 4918, and others, including informational oddities like 418 "I'm a teapot".

use std::sync::LazyLock;

use std::collections::HashMap;

static PHRASES: LazyLock<HashMap<u16, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        (100, "Continue"),
        (101, "Switching Protocols"),
        (102, "Processing"),
        (103, "Early Hints"),
        (200, "OK"),
        (201, "Created"),
        (202, "Accepted"),
        (203, "Non-Authoritative Information"),
        (204, "No Content"),
        (205, "Reset Content"),
        (206, "Partial Content"),
        (207, "Multi-Status"),
        (208, "Already Reported"),
        (226, "IM Used"),
        (300, "Multiple Choices"),
        (301, "Moved Permanently"),
        (302, "Found"),
        (303, "See Other"),
        (304, "Not Modified"),
        (305, "Use Proxy"),
        (307, "Temporary Redirect"),
        (308, "Permanent Redirect"),
        (400, "Bad Request"),
        (401, "Unauthorized"),
        (402, "Payment Required"),
        (403, "Forbidden"),
        (404, "Not Found"),
        (405, "Method Not Allowed"),
        (406, "Not Acceptable"),
        (407, "Proxy Authentication Required"),
        (408, "Request Timeout"),
        (409, "Conflict"),
        (410, "Gone"),
        (411, "Length Required"),
        (412, "Precondition Failed"),
        (413, "Payload Too Large"),
        (414, "URI Too Long"),
        (415, "Unsupported Media Type"),
        (416, "Range Not Satisfiable"),
        (417, "Expectation Failed"),
        (418, "I'm a teapot"),
        (421, "Misdirected Request"),
        (422, "Unprocessable Entity"),
        (423, "Locked"),
        (424, "Failed Dependency"),
        (425, "Too Early"),
        (426, "Upgrade Required"),
        (428, "Precondition Required"),
        (429, "Too Many Requests"),
        (431, "Request Header Fields Too Large"),
        (451, "Unavailable For Legal Reasons"),
        (500, "Internal Server Error"),
        (501, "Not Implemented"),
        (502, "Bad Gateway"),
        (503, "Service Unavailable"),
        (504, "Gateway Timeout"),
        (505, "HTTP Version Not Supported"),
        (506, "Variant Also Negotiates"),
        (507, "Insufficient Storage"),
        (508, "Loop Detected"),
        (510, "Not Extended"),
        (511, "Network Authentication Required"),
    ])
});

/// Returns the standard reason phrase for an HTTP status code, or `"Unknown Status Code"`
/// if the code is not in the lookup table. The table is initialized once on first call
/// and reused for the lifetime of the process.
pub fn status_text(code: u16) -> &'static str {
    PHRASES.get(&code).copied().unwrap_or("Unknown Status Code")
}
