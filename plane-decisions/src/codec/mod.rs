//! The wire words both dialects share: plain HTTP+JSON, the body relayed as the caller and the far
//! end wrote it.
//!
//! There is no envelope to read the way MCP/A2A read a JSON-RPC envelope: each dialect's decision
//! call is named by its HTTP verb and path (`ops.rs`), and a request or an answer is one JSON
//! document. This module names the small, closed set of pointers the plane reads (a request's
//! top-level `model`; an answer's usage counts, by dialect), never `state` and never `answers`, and
//! reads them through the contract's own span grammar rather than a second parser.
//!
//! ## What reaches the far end, precisely
//!
//! The caller's body reaches the far end as the caller sent it, with two exceptions. A configured
//! `upstream_model` that differs from the name the caller used is spliced over the top-level
//! `model` value (`driven::spliced`). A bound request-stage hook's rewrite replaces the request's
//! members wholesale with its document (`driven::rewritten`). With neither, every byte is the
//! caller's. The far end's answer reaches the caller unchanged; its counts are read as it passes.

/// The pointer a SUCCESSFUL `systemone` response reports its billable usage count at.
///
/// A `jev` success's decision count (`driven::FarEndReading::counts`; one decision, the plane's
/// floor, when it states none).
pub const PTR_USAGE_UNITS: &str = "/usage/units";

/// The pointers a SUCCESSFUL `systemone` response reports its input and output tokens at, where it
/// reports them.
pub const PTR_USAGE_INPUT_TOKENS: &str = "/usage/input_tokens";
/// See [`PTR_USAGE_INPUT_TOKENS`].
pub const PTR_USAGE_OUTPUT_TOKENS: &str = "/usage/output_tokens";

/// The pointer a SUCCESSFUL `nanojev` answer states the upstream's own billing unit at: the states
/// its evaluation ran (a success with no count bills the plane's floor and says so in one
/// `diag_warn` line).
pub const PTR_EXECUTION_STATES: &str = "/execution/states";

/// The pointer a request names the model it is routed over at: its top-level `model`. Routing
/// metadata, never decision content.
pub const PTR_MODEL: &str = "/model";

/// The raw bytes at one declared pointer of a body (the crate's tests' reference reader; the door
/// resolves its pointers where it reads them, so an unreadable body is never taken to be absent).
#[cfg(test)]
#[must_use]
pub fn read_raw<'u>(body: &'u [u8], pointer: &str) -> Option<&'u [u8]> {
    match busbar_contract::spans::resolve_pointer(body, pointer) {
        busbar_contract::spans::Resolved::Found(span) => body.get(span.start..span.end),
        _ => None,
    }
}

/// The envelope member naming the document type of an outbound body.
pub(crate) const FIELD_CONTENT_TYPE: &str = "content-type";

/// The response field a refusal names its wait in: whole seconds until the caller may retry.
pub(crate) const FIELD_RETRY_AFTER: &str = "retry-after";

/// The document type every body of this protocol is.
pub(crate) const CONTENT_TYPE_JSON: &[u8] = b"application/json";

/// Build the bare JSON error object jev's refusal/error shape is: `{"error": {"code": ..,
/// "message": ..}}`. Written by hand rather than through `serde_json::json!` so the byte order is
/// pinned and a conformance test can assert the exact bytes.
pub(crate) fn error_body(code: &str, message: &str) -> Vec<u8> {
    format!(
        "{{\"error\":{{\"code\":{},\"message\":{}}}}}",
        serde_json::to_string(code).unwrap_or_else(|_| "\"internal\"".to_string()),
        serde_json::to_string(message).unwrap_or_else(|_| "\"error\"".to_string()),
    )
    .into_bytes()
}

#[cfg(test)]
#[path = "../tests/codec.rs"]
mod tests;
