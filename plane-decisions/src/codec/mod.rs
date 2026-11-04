//! The wire words both dialects share: plain HTTP+JSON, the body relayed as the caller and the far

/// The pointer a SUCCESSFUL `systemone` response reports its billable usage count at.
pub const PTR_USAGE_UNITS: &str = "/usage/units";

/// The pointers a SUCCESSFUL `systemone` response reports its input and output tokens at, where it
pub const PTR_USAGE_INPUT_TOKENS: &str = "/usage/input_tokens";
/// See [`PTR_USAGE_INPUT_TOKENS`].
pub const PTR_USAGE_OUTPUT_TOKENS: &str = "/usage/output_tokens";

/// The pointer a SUCCESSFUL `nanojev` answer states the upstream's own billing unit at: the states
pub const PTR_EXECUTION_STATES: &str = "/execution/states";

/// The pointer a request names the model it is routed over at: its top-level `model`. Routing
pub const PTR_MODEL: &str = "/model";

/// The raw bytes at one declared pointer of a body (the crate's tests' reference reader; the door
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
