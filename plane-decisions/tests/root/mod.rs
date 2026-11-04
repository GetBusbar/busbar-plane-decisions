// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DECISIONS PLANE DOOR'S ROOT CELLS, as the composition root's door-cells row reads them: this
//! file is the one this crate states under its `links` name (`build.rs`, `cargo::metadata=cells=`),
//! compiled as a child of the composition root's serve module in its test builds (`super::` is
//! `root::serve`), the same for this pinned dependency as for a path one.

/// THIS PLANE'S CAPABILITY-EQUALITY COLUMNS (`qa/capability-equality.json`): the cells its root leg
/// answers for.
pub(crate) const COLUMNS: &[&str] = &["decisions"];

/// `(capability, column, reason)`: the root-leg cells this plane is NOT owed, each the ledger's own
/// not-applicable cell with its reason, word for word.
pub(crate) const NOT_APPLICABLE: &[(&str, &str, &str)] = &[
    (
        "failover-reroute",
        "decisions",
        "the decisions plane has no candidate set to walk: a request routes ROUTE_DIRECT to the one model it names (or, naming none, to the only model of its dialect), and a model is one provider entry; there is no second interchangeable candidate to try before the first byte",
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
        "the decisions plane publishes no listing a caller could SEE: it claims one operation per dialect a configured model uses (POST /v1/systemone for jev, POST /api/evaluate for nanojev) and nothing else, and GET /v1/models keeps the bytes the model catalogue's own plane serves, so there is no per-item visibility of its own to answer",
    ),
];

/// THE SECRET-SOURCE SCAN'S DRIVEN DOOR (the composition root's `secret_source.rs`, which holds
/// every linked door plane to a driver): the Statement names the cells in this file drive.
pub(crate) fn secret_source_driven() -> Vec<String> {
    Vec::new()
}

// The four hook integration cases and their private fixture scaffolding moved to
// dist/default/tests/product/decisions_hooks.rs in the actual distribution.
