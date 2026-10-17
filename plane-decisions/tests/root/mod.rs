// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DECISIONS PLANE DOOR'S ROOT CELLS, as the composition root's door-cells row reads them: this
//! file is the one this crate states under its `links` name (`build.rs`, `cargo::metadata=cells=`),
//! compiled as a child of the composition root's serve module in its test builds (`super::` is
//! `root::serve`), the same for this pinned dependency as for a path one. The kernel and the loader
//! are the root's to link, never a plane's, so this directory has no `main.rs`,
//! and `cargo test -p busbar-plane-decisions` does not build it.
//!
//! What it states is the plane's answer to busbar's capability-equality ledger: its column, and the
//! root-leg cells the ledger marks not applicable on it, reason for reason, which the composition
//! root holds equal to the ledger's (`root/tests/door_cells.rs`). The leg's proven cells are where
//! the ledger's `root_legs` row `root-decisions` names them (busbar's
//! `crates/busbar/src/root/tests/serve_tests.rs`); the door's secret-source driver is the
//! composition root's own (`root/tests/secret_source.rs`, through `decisions_name()`).

/// THIS PLANE'S CAPABILITY-EQUALITY COLUMNS (`qa/capability-equality.json`): the cells its root leg
/// answers for.
pub(crate) const COLUMNS: &[&str] = &["decisions"];

/// `(capability, column, reason)`: the root-leg cells this plane is NOT owed, each the ledger's own
/// not-applicable cell with its reason, word for word. The composition root holds the two equal: a
/// plane opts out of nothing the ledger says it has, and the ledger excuses nothing the plane does
/// not state.
pub(crate) const NOT_APPLICABLE: &[(&str, &str, &str)] = &[
    (
        "failover-reroute",
        "decisions",
        "the decisions plane has no candidate set to walk: a jev request names no model, so it is served only when decisions.models holds exactly one, and the door routes it ROUTE_DIRECT to that one model's provider; there is no second interchangeable candidate to try before the first byte",
    ),
    (
        "hooks-tap",
        "decisions",
        "Law 11 (BUSBAR-1.6.0.md: busbar does not interpret or act on the data it carries; coordinator ruling 2026-10-07): the decisions door declares no hook seat (busbar-plane-decisions lib.rs) and binds no hook view: its `project` is refused because the plane never reads the decision content, the PII witness (plane_door.rs, the `project` slot). Its tail states flags 0, never TAIL_HOOKS_GATED (plane_door.rs TAIL), and the serve path binds a plane's hook stage only for a tail that states it (root/serve.rs compose_planes_over). With no projection there is no payload for a tap to observe or transform. `decisions.hooks` stays accepted exactly as today: 1.5.5 (a71b21e9fa) had no decisions section at all (its DeployCfg is deny_unknown_fields and names none), so there is no 1.5.5 behaviour to keep or refuse.",
    ),
    (
        "hooks-gate",
        "decisions",
        "Law 11 (BUSBAR-1.6.0.md: busbar does not interpret or act on the data it carries; coordinator ruling 2026-10-07): the decisions door declares no hook seat (busbar-plane-decisions lib.rs) and binds no hook view (`project` refused, plane_door.rs), and its tail never states TAIL_HOOKS_GATED (plane_door.rs TAIL, flags 0), so the serve path binds no gate stage for it (root/serve.rs compose_planes_over binds a plane's gates and rewrites only for a tail that states the gate-first order). A gate decides over the projected request and this plane projects none, so there is nothing for hooks::gate to decide on. `decisions.hooks` stays accepted exactly as today: 1.5.5 (a71b21e9fa) had no decisions section at all (its DeployCfg is deny_unknown_fields and names none), so there is no 1.5.5 behaviour to keep or refuse.",
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
/// every linked door plane to a driver): the Statement names the cells in this file drive. None:
/// this door's driver is the composition root's own (see the module docs), so this file adds no
/// name to the scan.
pub(crate) fn secret_source_driven() -> Vec<String> {
    Vec::new()
}
