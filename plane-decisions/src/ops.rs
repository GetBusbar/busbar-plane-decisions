//! The operation vocabulary: which `(method, path)` is which priced operation.

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
    MethodRow {
        op: OP_CLASSIFY,
        verb: "POST",
        path: crate::classification::PATH,
    },
];

/// Which row a `(verb, path)` pair names, where one does.
#[must_use]
pub fn row_for(verb: &str, path: &str) -> Option<&'static MethodRow> {
    METHODS
        .iter()
        .find(|r| r.path == path && r.verb.eq_ignore_ascii_case(verb))
}

#[cfg(test)]
#[path = "tests/ops.rs"]
mod tests;

/// Versioned plugin-level text classification.
pub const OP_CLASSIFY: OpClassId = OpClassId::new("classify_v1");
