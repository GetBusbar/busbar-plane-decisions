// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DECISIONS PLANE'S ROOT CELLS, stated by the plane (busbar #1006: every linked plane door
//! states its root cells; ARCHITECT ruling on #464, the composition root names no plane).
//!
//! This file is compiled by busbar's composition root, never by this repo: the root's `build.rs`
//! reads the path this crate's `build.rs` states under its `links` name
//! (`DEP_BUSBAR_DOOR_CELLS_<CRATE>_CELLS`) and includes it as a child of its `serve` module in its
//! test build only (`super::` is `root::serve`, `crate::` is the busbar binary). The plane's own
//! `cargo test` does not build it: there is no `main.rs` here, and the kernel and the loader are the
//! root's to link, never a plane's.
//!
//! What it states:
//! - [`COLUMNS`]: the plane's capability-equality columns (`qa/capability-equality.json` `planes`).
//! - [`NOT_APPLICABLE`]: the root-leg cells the ledger marks not applicable on those columns, each
//!   with the ledger's reason word for word. The root holds the two equal, both ways.
//! - [`secret_source_driven`]: the Statement names these cells drive through the root's
//!   secret-source scan. None today: the root's `secret_source.rs` drives this door itself.
//!
//! The plane's served root cells (the ledger's `root` evidence) are still the composition root's
//! files. They move here, with their ledger citations, once the root resolves a pinned door's cells
//! from this same `links` output.

use std::sync::Arc;

use busbar_contract::abi::mechanism::door::DoorFn;

use crate::root::loader::dispatch::kinds::plane::Plane;
use crate::root::loader::dispatch::{load_linked, Bind, LinkedRow, NoSink};

/// The config section this plane's door owns: how its linked door is found among the root's.
const SECTION: &str = "decisions";

/// THIS PLANE'S CAPABILITY-EQUALITY COLUMNS (`qa/capability-equality.json`): the cells its root leg
/// answers for.
pub(crate) const COLUMNS: &[&str] = &["decisions"];

/// `(capability, column, reason)`: the root-leg cells this plane is NOT owed, each the ledger's own
/// not-applicable cell with its reason, word for word. The composition root holds the two equal: a
/// plane opts out of nothing the ledger says it has, and the ledger excuses nothing the plane does
/// not state.
pub(crate) const NOT_APPLICABLE: &[(&str, &str, &str)] = &[
    (
        "catalogue",
        "decisions",
        "the decisions plane publishes no listing a caller could SEE: it claims one operation (POST /v1/systemone) and nothing else, and GET /v1/models keeps the bytes the llm plane's catalogue serves, so there is no per-item visibility of its own to answer",
    ),
    (
        "failover-reroute",
        "decisions",
        "the decisions plane has no candidate set to walk: a jev request names no model, so it is served only when decisions.models holds exactly one, and the door routes it ROUTE_DIRECT to that one model's provider; there is no second interchangeable candidate to try before the first byte",
    ),
    (
        "hooks-gate",
        "decisions",
        "Law 11 (BUSBAR-1.6.0.md: busbar does not interpret or act on the data it carries; coordinator ruling 2026-10-07): the decisions door declares no hook seat (busbar-plane-decisions lib.rs) and binds no hook view (`project` refused, plane_door.rs), and its tail never states TAIL_HOOKS_GATED (plane_door.rs TAIL, flags 0), so the serve path binds no gate stage for it (root/serve.rs compose_planes_over binds a plane's gates and rewrites only for a tail that states the gate-first order). A gate decides over the projected request and this plane projects none, so there is nothing for hooks::gate to decide on. `decisions.hooks` stays accepted exactly as today: 1.5.5 (a71b21e9fa) had no decisions section at all (its DeployCfg is deny_unknown_fields and names none), so there is no 1.5.5 behaviour to keep or refuse.",
    ),
    (
        "hooks-tap",
        "decisions",
        "Law 11 (BUSBAR-1.6.0.md: busbar does not interpret or act on the data it carries; coordinator ruling 2026-10-07): the decisions door declares no hook seat (busbar-plane-decisions lib.rs) and binds no hook view: its `project` is refused because the plane never reads the decision content, the PII witness (plane_door.rs, the `project` slot). Its tail states flags 0, never TAIL_HOOKS_GATED (plane_door.rs TAIL), and the serve path binds a plane's hook stage only for a tail that states it (root/serve.rs compose_planes_over). With no projection there is no payload for a tap to observe or transform. `decisions.hooks` stays accepted exactly as today: 1.5.5 (a71b21e9fa) had no decisions section at all (its DeployCfg is deny_unknown_fields and names none), so there is no 1.5.5 behaviour to keep or refuse.",
    ),
    (
        "net-guard",
        "decisions",
        "the decisions plane's only outbound destination is its model's operator-configured provider base_url; no request-authored URL reaches its egress (the jev body is relayed, never read for a destination), so there is no SSRF surface the plane derives",
    ),
    (
        "trust-pinning",
        "decisions",
        "decision providers are operator-declared config endpoints (a providers: entry named by decisions.models), not peer-published artifacts; there is no upstream-authored schema or card to pin, so the trust lifecycle has nothing of this plane's to govern",
    ),
];

/// THE SECRET-SOURCE SCAN'S DRIVEN DOOR NAMES these cells contribute: none. The composition root's
/// `secret_source.rs` drives this door itself, so the scan already holds it to a driver.
pub(crate) fn secret_source_driven() -> Vec<String> {
    Vec::new()
}

/// THE DECISIONS DOOR: of the root's linked plane doors, the one whose Statement declares
/// [`SECTION`] (found by binding each, once). `None` is a defect, never a skip.
fn linked_door() -> Option<DoorFn> {
    static DOOR: std::sync::OnceLock<Option<DoorFn>> = std::sync::OnceLock::new();
    *DOOR.get_or_init(|| {
        let dispatcher = Arc::new(crate::root::loader::dispatch::Dispatcher::new(
            crate::root::loader::dispatch::DispatchConfig::default(),
        ));
        crate::LINKED.plane_doors.iter().copied().find(|&door| {
            let Ok(row) = LinkedRow::of(door) else {
                return false;
            };
            load_linked::<Plane>(
                &row,
                Bind {
                    instance: Arc::from("decisions-door-probe"),
                    max_inflight_cap: 64,
                    sink: Arc::new(NoSink),
                    dispatcher: dispatcher.adopter(),
                    conns: crate::root::loader::dispatch::ConnTable::Probe,
                },
            )
            .is_ok_and(|plane| plane.served().section == SECTION)
        })
    })
}

/// THE DOOR IS LINKED AND STATES ITSELF: the root links a plane door owning [`SECTION`], and its
/// Statement reads with a name. The cells above answer for that door, and no other.
#[test]
fn the_root_links_the_door_these_cells_answer_for() {
    let door = linked_door().expect("the root links a plane door owning the decisions section");
    let row = LinkedRow::of(door).expect("the door states itself");
    let statement = busbar_contract::abi::mechanism::rendering::read(&row.statement)
        .expect("its Statement reads");
    assert!(
        !statement.name.is_empty(),
        "the door owning `{SECTION}` states no name"
    );
}

/// THE COLUMNS ARE THIS PLANE'S, AND EVERY NOT-APPLICABLE CELL IS ON ONE OF THEM: a cell stated on
/// another plane's column, or a column stated twice, is a statement about the wrong door.
#[test]
fn the_not_applicable_cells_are_on_this_planes_columns() {
    assert!(
        !COLUMNS.is_empty(),
        "the plane states no capability-equality column"
    );
    let mut seen: Vec<&str> = Vec::new();
    for column in COLUMNS {
        assert!(!seen.contains(column), "column `{column}` is stated twice");
        seen.push(*column);
    }
    for (capability, column, reason) in NOT_APPLICABLE {
        assert!(
            COLUMNS.contains(column),
            "`{capability}` is excused on `{column}`, which is not this plane's column"
        );
        assert!(
            !reason.trim().is_empty(),
            "`{capability}` x `{column}` is excused with no reason"
        );
    }
    assert!(
        secret_source_driven().is_empty(),
        "the root's secret-source scan drives this door itself"
    );
}
