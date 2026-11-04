// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DECISIONS PLANE DOOR'S ROOT CELLS, as the composition root's door-cells row reads them: this

/// THIS PLANE'S CAPABILITY-EQUALITY COLUMNS (`qa/capability-equality.json`): the cells its root leg
pub(crate) const COLUMNS: &[&str] = &["decisions"];

/// `(capability, column, reason)`: the root-leg cells this plane is NOT owed, each the ledger's own
pub(crate) const NOT_APPLICABLE: &[(&str, &str, &str)] = &[
    (
        "failover-reroute",
        "decisions",
        "the decisions plane has no candidate set to walk: a jev request names no model, so it is served only when decisions.models holds exactly one, and the door routes it ROUTE_DIRECT to that one model's provider; there is no second interchangeable candidate to try before the first byte",
    ),
    (
        "trust-pinning",
        "decisions",
        "decision providers are operator-declared config endpoints (a providers: entry named by decisions.models), not peer-published artifacts; there is no upstream-authored schema or card to pin, so the trust lifecycle has nothing of this plane's to govern",
    ),
    (
        "net-guard",
        "decisions",
        "the decisions plane's only outbound destination is its model's operator-configured provider base_url; no request-authored URL reaches its egress (the jev body is relayed, never read for a destination), so there is no SSRF surface the plane derives",
    ),
    (
        "catalogue",
        "decisions",
        "the decisions plane publishes no listing a caller could SEE: it claims one operation (POST /v1/systemone) and nothing else, and GET /v1/models keeps the bytes the model catalogue's own plane serves, so there is no per-item visibility of its own to answer",
    ),
];

/// THE SECRET-SOURCE SCAN'S DRIVEN DOOR (the composition root's `secret_source.rs`, which holds
pub(crate) fn secret_source_driven() -> Vec<String> {
    Vec::new()
}

// The four hook integration cases and their private fixture scaffolding moved to
// dist/default/tests/product/decisions_hooks.rs in the actual distribution.
