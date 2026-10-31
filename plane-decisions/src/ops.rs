//! The operation vocabulary: which `(method, path)` is which priced operation.
//!
//! The plane serves one operation per dialect, each its dialect's decision call: `jev`'s
//! `POST /v1/systemone` and `nanojev`'s `POST /api/evaluate`. Each relays the caller's body to the
//! far end unchanged but for the governed top-level `model` splice (`upstream_model`, and a hook's
//! rewrite where one is bound). Unlike MCP/A2A, which name their operation in a JSON-RPC method
//! member, these dialects name it in the HTTP verb and path, so the table below is keyed on those.
//! `GET /v1/models` is no operation of this plane: that path keeps the bytes busbar's model list
//! serves there.

use busbar_contract::ids::OpClassId;

/// `POST /v1/systemone` — the decision call. Priced on the request it carries and metered on the
/// usage the success response reports (billable on success only).
pub const OP_SYSTEMONE: OpClassId = OpClassId::new("systemone");

/// `POST /api/evaluate` — the decision call of the second dialect, `nanojev`.
pub const OP_EVALUATE: OpClassId = OpClassId::new("evaluate");

/// The path jev's decision call is mounted on.
pub const PATH_SYSTEMONE: &str = "/v1/systemone";

/// The path the `nanojev` dialect's decision call is mounted on.
pub const PATH_EVALUATE: &str = "/api/evaluate";

/// One row of the method table: which `(verb, path)` is which operation. Its `(verb, path)` keys
/// are the claim table's ([`crate::driven::tail::CLAIMS`]), in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodRow {
    /// The operation class this row declares.
    pub op: OpClassId,
    /// The HTTP verb the request arrives as.
    pub verb: &'static str,
    /// The path the request arrives on.
    pub path: &'static str,
}

/// The method table, in declaration order.
pub const METHODS: &[MethodRow] = &[
    MethodRow {
        op: OP_SYSTEMONE,
        verb: "POST",
        path: PATH_SYSTEMONE,
    },
    MethodRow {
        op: OP_EVALUATE,
        verb: "POST",
        path: PATH_EVALUATE,
    },
];

/// Which row a `(verb, path)` pair names, where one does.
///
/// The path match is exact (each decision call is a fixed path, not a pattern family the way
/// A2A's task collection is), so there is no most-specific-wins question to settle here.
#[must_use]
pub fn row_for(verb: &str, path: &str) -> Option<&'static MethodRow> {
    METHODS
        .iter()
        .find(|r| r.path == path && r.verb.eq_ignore_ascii_case(verb))
}

#[cfg(test)]
#[path = "tests/ops.rs"]
mod tests;
