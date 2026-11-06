//! The wire words both dialects share: plain HTTP+JSON, the body relayed as the caller and the far
//! end wrote it.

/// The pointer a SUCCESSFUL `systemone` response reports its billable usage count at.
pub const PTR_USAGE_UNITS: &str = "/usage/units";

/// The pointers a SUCCESSFUL `systemone` response reports its input and output tokens at, where it
/// reports them.
pub const PTR_USAGE_INPUT_TOKENS: &str = "/usage/input_tokens";
/// See [`PTR_USAGE_INPUT_TOKENS`].
pub const PTR_USAGE_OUTPUT_TOKENS: &str = "/usage/output_tokens";

/// The pointer a SUCCESSFUL `nanojev` answer states the upstream's own billing unit at: the states
/// its evaluation ran (a success with no count bills the plane's floor and raises 5029).
pub const PTR_EXECUTION_STATES: &str = "/execution/states";

/// The pointer a request names the model it is routed over at: its top-level `model`. Routing
/// metadata, never decision content.
pub const PTR_MODEL: &str = "/model";

/// The envelope member naming the document type of an outbound body.
pub(crate) const FIELD_CONTENT_TYPE: &str = "content-type";

/// The response field a refusal names its wait in: whole seconds until the caller may retry.
pub(crate) const FIELD_RETRY_AFTER: &str = "retry-after";

/// The document type every body of this protocol is.
pub(crate) const CONTENT_TYPE_JSON: &[u8] = b"application/json";

/// Build the bare JSON error object jev's refusal/error shape is: `{"error": {"code": ..,
/// "message": ..}}`.
pub(crate) fn error_body(code: &str, message: &str) -> Vec<u8> {
    format!(
        "{{\"error\":{{\"code\":{},\"message\":{}}}}}",
        serde_json::Value::from(code),
        serde_json::Value::from(message),
    )
    .into_bytes()
}

#[cfg(test)]
#[path = "../tests/codec.rs"]
pub(crate) mod tests;
