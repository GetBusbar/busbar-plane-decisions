// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DOOR'S ANSWERS, DRIVEN OVER ITS ABI as the kernel's plane driver drives them.
//!
//! * MODEL RESOLUTION (THE DESIGN section 2, the decisions bullet; DECISIONS D8b): "a request routes
//!   by its model; one configured model and none named is the default (byte-identical); more than
//!   one and none named is 400; an unknown model is 404." The claim is published whenever a model is
//!   configured.
//! * `upstream_model` (the same bullet): "rewrites only the top-level model value, by span splice,
//!   when it is set and differs; otherwise the body passes through."

#![allow(unsafe_code)]

mod door_harness;

use busbar_contract::abi::mechanism::call::Outcome;
use busbar_contract::abi::plane::{PRINCIPAL_REQUIRED, ROUTE_DIRECT};
use door_harness::{claims, open, Bufs, Given};

const SYSTEMONE: (&str, &str) = ("POST", "/v1/systemone");

const TWO: &str = r#"{"models":{"a":{"provider":"typesafe"},"b":{"provider":"typesafe"}}}"#;
const ONE: &str = r#"{"models":{"jev":{"provider":"typesafe"}}}"#;

/// RED (audit plane-decisions#1): several configured models publish the claim; none publishes none.
#[test]
fn every_generation_with_a_model_claims_systemone() {
    assert_eq!(open(TWO).claims, claims(&[SYSTEMONE]), "two models");
    assert_eq!(open(ONE).claims, claims(&[SYSTEMONE]), "one model");
    let none = open(r#"{"models":{}}"#);
    assert!(none.claims.is_empty(), "no model claims nothing");
    // A refresh onto several models keeps the claim, and onto none drops it.
    assert_eq!(none.refresh(TWO, 2), claims(&[SYSTEMONE]));
    assert!(none.refresh("{}", 3).is_empty());
}

/// RED (audit plane-decisions#1): a request routes over the model it names; several configured and
/// none named is 400; a name no model has is 404; one configured and none named is the default.
#[test]
fn a_request_routes_by_its_model() {
    let two = open(TWO);
    let b = two.arrive(1, br#"{"model":"b","state":{}}"#);
    assert_eq!(
        (b.outcome, b.route, b.entry.as_str()),
        (Outcome::Ready, ROUTE_DIRECT, "b"),
        "{b:?}"
    );
    // A decision is billed to someone: a principal is verified before the first piece.
    assert_eq!(b.principal_need, PRINCIPAL_REQUIRED, "{b:?}");
    let a = two.arrive(2, br#"{"state":{},"model":"a"}"#);
    assert_eq!(
        (a.outcome, a.entry.as_str()),
        (Outcome::Ready, "a"),
        "{a:?}"
    );

    let unnamed = two.arrive(3, br#"{"state":{}}"#);
    assert_eq!(
        (unnamed.outcome, unnamed.status),
        (Outcome::Refused, 400),
        "{unnamed:?}"
    );
    assert!(
        unnamed.refusal != 0 && !unnamed.text.is_empty(),
        "{unnamed:?}"
    );

    let unknown = two.arrive(4, br#"{"model":"zz","state":{}}"#);
    assert_eq!(
        (unknown.outcome, unknown.status),
        (Outcome::Refused, 404),
        "{unknown:?}"
    );
    assert!(unknown.text.contains("'zz'"), "{unknown:?}");

    let not_a_name = two.arrive(5, br#"{"model":7}"#);
    assert_eq!(
        (not_a_name.outcome, not_a_name.status),
        (Outcome::Refused, 400),
        "{not_a_name:?}"
    );

    let one = open(ONE);
    let default = one.arrive(6, br#"{"state":{}}"#);
    assert_eq!(
        (default.outcome, default.route, default.entry.as_str()),
        (Outcome::Ready, ROUTE_DIRECT, "jev"),
        "{default:?}"
    );
    let named = one.arrive(7, br#"{"model":"jev"}"#);
    assert_eq!(
        (named.outcome, named.entry.as_str()),
        (Outcome::Ready, "jev")
    );
    let elsewhere = one.arrive(8, br#"{"model":"other"}"#);
    assert_eq!(
        (elsewhere.outcome, elsewhere.status),
        (Outcome::Refused, 404),
        "{elsewhere:?}"
    );
}

/// THE DROPPED-IN TWIN NAMES A ROUTE AS THE LINKED DOOR DOES (THE DESIGN section 11.4, "compiled in
/// or dropped in ... NOTHING CHANGES about the plugin itself"; section 11.2, "no allocation and no
/// blocking on the hot path"). The twin's image is built from this workspace against its own
/// busbar-contract pin, so the SDK compiled here is the SDK the shipped library carries. A READY
/// arrival naming a model the instance has named before is answered from the instance's one kept
/// copy of that name, read with no heap and no lock: every arrival over model `b` hands the host the
/// same name at the same address. An SDK that keeps a fresh copy of the name per arrival (a heap
/// allocation and a push under the instance's shared text lock) hands a new address each time.
#[test]
fn a_model_named_again_is_routed_by_the_instances_kept_name() {
    let two = open(TWO);
    let first = two.arrive(1, br#"{"model":"b","state":{}}"#);
    assert_eq!(
        (first.outcome, first.route, first.entry.as_str()),
        (Outcome::Ready, ROUTE_DIRECT, "b"),
        "{first:?}"
    );
    for unit in 2..=4 {
        let again = two.arrive(unit, br#"{"state":{},"model":"b"}"#);
        assert_eq!(
            (again.outcome, again.entry.as_str()),
            (Outcome::Ready, "b"),
            "{again:?}"
        );
        assert_eq!(
            again.entry_at, first.entry_at,
            "arrival {unit} named model b from a fresh copy, not the instance's kept name"
        );
    }
    let other = two.arrive(5, br#"{"model":"a","state":{}}"#);
    assert_eq!(other.entry.as_str(), "a", "{other:?}");
    assert_ne!(
        other.entry_at, first.entry_at,
        "each entry keeps its own name"
    );
}

/// A refused arrival is answered in jev's error shape, the arrival's words and status.
#[test]
fn an_unrouted_arrival_renders_in_the_dialect_shape() {
    let two = open(TWO);
    let unknown = two.arrive(1, br#"{"model":"zz"}"#);
    let (outcome, body) = two.refusal(1, unknown.status, &unknown.text, true);
    assert_eq!(outcome, Outcome::Ready);
    let v: serde_json::Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(v["error"]["message"], unknown.text.as_str(), "{v}");
    assert!(v["error"]["code"].is_string(), "{v}");
}

/// The caller's body as the far end receives it, for a unit routed over `settings` by `body`.
fn sent(settings: &str, body: &[u8]) -> String {
    let door = open(settings);
    let arrived = door.arrive(1, body);
    assert_eq!(arrived.outcome, Outcome::Ready, "{arrived:?}");
    let mut bufs = Bufs::new(1 << 16);
    let attempt = door.piece(1, Given::attempt(1), &mut bufs);
    assert_eq!(attempt.request, "POST /v1/systemone");
    let caller = door.piece(1, Given::caller(body), &mut bufs);
    assert!(caller.far, "the caller's body goes to the far end");
    String::from_utf8(caller.bytes).expect("UTF-8")
}

/// RED (audit plane-decisions#2): a set `upstream_model` that differs replaces the top-level model
/// value alone; every other byte (member order, whitespace) is the caller's.
#[test]
fn upstream_model_is_spliced_into_the_top_level_model() {
    let upstream = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"}}}"#;
    let body = "{ \"state\" : {\"model\":\"inner\"},\n  \"model\" :  \"jev\" , \"n\":[1, 2] }";
    assert_eq!(
        sent(upstream, body.as_bytes()),
        "{ \"state\" : {\"model\":\"inner\"},\n  \"model\" :  \"jev-1.13.0\" , \"n\":[1, 2] }"
    );
    // The same name, or none set, or a body that names no model: the body passes through.
    let same = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev"}}}"#;
    assert_eq!(sent(same, body.as_bytes()), body);
    assert_eq!(sent(ONE, body.as_bytes()), body);
    let unnamed = "{\"state\":{}}";
    assert_eq!(sent(upstream, unnamed.as_bytes()), unnamed);
}

/// One unit served over [`ONE`]: its ATTEMPT, the caller's body, and the far end's whole answer,
/// `status` with `answer`. Answers the far end's piece.
fn answered(status: u32, answer: &[u8]) -> door_harness::Answered {
    let door = open(ONE);
    assert_eq!(door.arrive(1, br#"{"state":{}}"#).outcome, Outcome::Ready);
    let mut bufs = Bufs::new(1 << 16);
    door.piece(1, Given::attempt(1), &mut bufs);
    door.piece(1, Given::caller(br#"{"state":{}}"#), &mut bufs);
    door.piece(1, Given::whole(status, answer), &mut bufs)
}

/// The door's Statement tail, read off the Statement `door()` states, as the loader reads it.
fn tail() -> &'static busbar_contract::abi::plane::PlaneTail {
    // SAFETY: the door, its Statement and a plane Statement's kind tail (a `PlaneTail`) are
    // `'static`.
    unsafe {
        &*(*(*busbar_plane_decisions::door()).statement())
            .kind_tail
            .cast::<busbar_contract::abi::plane::PlaneTail>()
    }
}

/// The tail's billable classes.
fn classes() -> &'static [busbar_contract::abi::plane::BillableClass] {
    let tail = tail();
    // SAFETY: the tail's billable classes are a `'static` list of `billable_classes_len` entries,
    // each naming `'static` strings.
    unsafe { std::slice::from_raw_parts(tail.billable_classes_ptr(), tail.billable_classes_len()) }
}

/// The class index the tail names `name` at.
fn class(name: &str) -> u32 {
    let named = |c: &busbar_contract::abi::plane::BillableClass| {
        // SAFETY: as above.
        unsafe { c.class.bytes() == name.as_bytes() }
    };
    let at = classes()
        .iter()
        .position(named)
        .unwrap_or_else(|| panic!("the tail declares no class `{name}`"));
    u32::try_from(at).unwrap()
}

/// `(class, amount)` of every count an answer bills, each REPORTED or the plane's FLOOR.
fn counts(a: &door_harness::Answered) -> Vec<(u32, u64)> {
    assert!(
        a.units.iter().all(|(_, source, _)| {
            *source == busbar_contract::abi::plane::UNITS_REPORTED
                || *source == busbar_contract::abi::plane::UNITS_FLOOR
        }),
        "{a:?}"
    );
    a.units.iter().map(|(c, _, n)| (*c, *n)).collect()
}

/// RED (audit plane-decisions#4): THE DESIGN section 7, "bills one `decision` per successful, settled
/// answer from every provider": a success with no count bills one decision, beside its fee.
#[test]
fn a_success_with_no_count_bills_one_decision() {
    let fee = class(busbar_contract::plane::PER_REQUEST);
    let a = answered(200, br#"{"answers":{}}"#);
    assert_eq!(a.outcome, Some(Outcome::Ready), "{a:?}");
    assert_eq!(counts(&a), vec![(class("decision"), 1), (fee, 1)]);
    // RED (audit plane-decisions R3, the count's source): the decision the answer did not state is
    // the plane's floor, never a count the far end reported; the request fee is reported.
    assert_eq!(
        a.units,
        vec![
            (
                class("decision"),
                busbar_contract::abi::plane::UNITS_FLOOR,
                1
            ),
            (fee, busbar_contract::abi::plane::UNITS_REPORTED, 1),
        ]
    );
    let reported = answered(200, br#"{"usage":{"units":42}}"#);
    assert_eq!(counts(&reported), vec![(class("decision"), 42), (fee, 1)]);
}

/// RED (audit plane-decisions#4): one success rule: a non-2xx answer bills nothing, whatever count
/// it carries.
#[test]
fn a_non_success_bills_nothing() {
    let a = answered(500, br#"{"usage":{"units":2}}"#);
    assert_eq!(a.outcome, Some(Outcome::Ready), "{a:?}");
    assert!(a.units.is_empty(), "{a:?}");
    assert_eq!(a.bytes, br#"{"usage":{"units":2}}"#, "relayed unchanged");
}

/// RED (owner ruling 2026-10-27, spend is always attributed and nothing is silent): a success
/// whose count is not a whole count bills the plane's floor (one decision, `UNITS_FLOOR`) and its
/// fee, relays the answer unchanged, and says so in one log line; a unit that succeeded is never
/// failed. RED before: the unit FAILED and billed nothing.
#[test]
fn a_success_with_an_unreadable_count_bills_the_floor_and_says_so() {
    let fee = class(busbar_contract::plane::PER_REQUEST);
    for answer in [
        &br#"{"usage":{"units":7.5}}"#[..],
        br#"{"usage":{"units":"7"}}"#,
        br#"{"usage":{"units":-3}}"#,
    ] {
        let a = answered(200, answer);
        let shown = String::from_utf8_lossy(answer);
        assert_eq!(
            (a.outcome, a.short),
            (Some(Outcome::Ready), false),
            "{shown}: {a:?}"
        );
        assert_eq!(
            a.units,
            vec![
                (
                    class("decision"),
                    busbar_contract::abi::plane::UNITS_FLOOR,
                    1
                ),
                (fee, busbar_contract::abi::plane::UNITS_REPORTED, 1),
            ],
            "{shown}"
        );
        assert_eq!(a.bytes, answer, "relayed unchanged");
        assert_eq!(
            a.logs
                .iter()
                .filter(|l| l.contains("usage is unreadable"))
                .count(),
            1,
            "{shown}: {:?}",
            a.logs
        );
    }
}

/// RED (audit plane-decisions#5): THE DESIGN section 7, "its card prices `decision`, `input_tokens`
/// and `output_tokens` (family `decision`, all three required, 0 = free)": the tail declares the
/// three in family `decision`, and a success's reported tokens are billed in them.
#[test]
fn the_tokens_a_success_reports_are_billed_in_family_decision() {
    let family = |name: &str| {
        let c = &classes()[class(name) as usize];
        // SAFETY: as in `classes`.
        String::from_utf8_lossy(unsafe { c.family.bytes() }).into_owned()
    };
    for name in ["decision", "input_tokens", "output_tokens"] {
        assert_eq!(family(name), "decision", "{name}");
    }
    let a = answered(
        200,
        br#"{"usage":{"units":2,"input_tokens":12,"output_tokens":5}}"#,
    );
    let got = counts(&a);
    for want in [
        (class("decision"), 2),
        (class("input_tokens"), 12),
        (class("output_tokens"), 5),
    ] {
        assert!(got.contains(&want), "{want:?} in {got:?}");
    }
}

/// The most units the door keeps state for at once (its `MAX_UNITS`, pinned to this value in-crate by
/// `the_unit_ceiling_is_the_one_the_door_tests_drive`).
const MAX_UNITS: u64 = 4096;

/// A live unit over [`ONE`]: arrived, its ATTEMPT crossed.
fn live(door: &door_harness::Door, unit: u64, bufs: &mut Bufs) {
    let arrived = door.arrive(unit, br#"{"state":{}}"#);
    assert_eq!(arrived.outcome, Outcome::Ready, "unit {unit}: {arrived:?}");
    let attempt = door.piece(unit, Given::attempt(1), bufs);
    assert_eq!(attempt.request, "POST /v1/systemone", "unit {unit}");
}

/// Whether `unit`'s body still reaches the far end: its ATTEMPT's state is still held.
fn still_live(door: &door_harness::Door, unit: u64, bufs: &mut Bufs) -> bool {
    door.piece(unit, Given::caller(br#"{"state":{}}"#), bufs)
        .far
}

/// RED (audit plane-decisions#6): a refusal ends its unit and drops its state, so refused traffic
/// never crowds out a live unit.
#[test]
fn a_refused_unit_is_dropped_and_a_live_unit_kept() {
    let door = open(ONE);
    let mut bufs = Bufs::new(1 << 16);
    live(&door, 1, &mut bufs);
    for unit in 2..2 + MAX_UNITS {
        assert_eq!(
            door.arrive(unit, br#"{}"#).outcome,
            Outcome::Ready,
            "unit {unit}"
        );
        door.refusal(unit, 429, "the key's limit is spent", false);
    }
    assert!(still_live(&door, 1, &mut bufs), "the live unit was dropped");
}

/// RED (audit plane-decisions#6): a cancel ends the unit on its ticket and drops its state.
#[test]
fn a_cancelled_unit_is_dropped_and_a_live_unit_kept() {
    let door = open(ONE);
    let mut bufs = Bufs::new(1 << 16);
    live(&door, 1, &mut bufs);
    for unit in 2..2 + MAX_UNITS {
        live(&door, unit, &mut bufs);
        let (outcome, disposition) = door.cancel(door_harness::ticket_of(unit));
        assert_eq!(
            (outcome, disposition),
            (Outcome::Ready, busbar_contract::abi::plane::CANCEL_ABORTED),
            "unit {unit}"
        );
    }
    assert!(still_live(&door, 1, &mut bufs), "the live unit was dropped");
}

/// RED (audit plane-decisions#6): a unit cut after its far end answered was not aborted before it.
#[test]
fn a_unit_cancelled_after_its_far_end_answered_is_cut_partway() {
    let door = open(ONE);
    let mut bufs = Bufs::new(1 << 16);
    live(&door, 1, &mut bufs);
    assert!(still_live(&door, 1, &mut bufs));
    let first = door.piece(1, Given::far(Some(200), false, br#"{"usage":"#), &mut bufs);
    assert_eq!(first.outcome, Some(Outcome::Ready));
    assert_eq!(
        door.cancel(door_harness::ticket_of(1)),
        (
            Outcome::Ready,
            busbar_contract::abi::plane::CANCEL_OK_PARTIAL
        )
    );
}

/// RED (audit plane-decisions#6): past [`MAX_UNITS`] live units a new arrival is refused, about
/// the entry it routed over, and no live unit is dropped to make room.
#[test]
fn past_the_cap_an_arrival_is_refused_and_no_live_unit_dropped() {
    let door = open(ONE);
    let mut bufs = Bufs::new(1 << 16);
    for unit in 1..=MAX_UNITS {
        live(&door, unit, &mut bufs);
    }
    let over = door.arrive(MAX_UNITS + 1, br#"{"state":{}}"#);
    assert_eq!(
        (over.outcome, over.status, over.route, over.entry.as_str()),
        (Outcome::Refused, 503, ROUTE_DIRECT, "jev"),
        "{over:?}"
    );
    assert!(
        still_live(&door, 1, &mut bufs),
        "the first live unit was dropped"
    );
    assert!(still_live(&door, MAX_UNITS, &mut bufs));
}

/// RED (audit plane-decisions R3, the reclaim): a unit that arrived and has crossed no piece yet
/// (its verify still running) is live: however many arrivals follow it, it is never dropped to make
/// room. Past [`MAX_UNITS`] live units the new arrival is refused, and the waiting unit is still
/// served at its first piece (THE DESIGN section 1: an active unit is never evicted; a bound is a
/// refusal at admission). Every unit that ends is released by its refusal, its last piece or its
/// cancel, which names it.
#[test]
fn a_live_unit_not_yet_started_is_never_dropped_to_make_room() {
    let door = open(ONE);
    for unit in 1..=MAX_UNITS {
        assert_eq!(door.arrive(unit, br#"{}"#).outcome, Outcome::Ready);
    }
    let next = door.arrive(MAX_UNITS + 1, br#"{}"#);
    assert_eq!(
        (next.outcome, next.status),
        (Outcome::Refused, 503),
        "{next:?}"
    );
    let mut bufs = Bufs::new(1 << 16);
    let first = door.piece(1, Given::attempt(1), &mut bufs);
    assert_eq!(
        (first.outcome, first.request.as_str()),
        (Some(Outcome::Ready), "POST /v1/systemone"),
        "the first unit, which crossed no piece while the instance filled, was dropped: {first:?}"
    );
    // A unit the host cancels by name makes room for the next arrival.
    assert_eq!(
        door.cancel_unit(2),
        (Outcome::Ready, busbar_contract::abi::plane::CANCEL_ABORTED)
    );
    assert_eq!(door.arrive(MAX_UNITS + 2, br#"{}"#).outcome, Outcome::Ready);
}

/// RED (audit plane-decisions R3, the cancel): the host cancels a unit it minted by naming it
/// (`PlaneCancelIn::unit`, "so a plane that keys its units from `arrive` finds one whose ticket no
/// piece ever carried"); a unit cancelled before its first piece is dropped, its relayed fields and
/// `upstream_model` with it, so its next piece is refused.
#[test]
fn a_unit_cancelled_before_its_first_piece_is_dropped() {
    let door = open(ONE);
    assert_eq!(door.arrive(5, br#"{"state":{}}"#).outcome, Outcome::Ready);
    assert_eq!(
        door.cancel_unit(5),
        (Outcome::Ready, busbar_contract::abi::plane::CANCEL_ABORTED)
    );
    let mut bufs = Bufs::new(1 << 16);
    let after = door.piece(5, Given::attempt(1), &mut bufs);
    assert_eq!(
        after.outcome,
        Some(Outcome::Refused),
        "the cancelled unit kept its state: {after:?}"
    );
    // A unit no other cancel names is untouched.
    assert_eq!(door.arrive(6, br#"{"state":{}}"#).outcome, Outcome::Ready);
    assert_eq!(
        door.cancel_unit(7),
        (Outcome::Ready, busbar_contract::abi::plane::CANCEL_ABORTED)
    );
    assert_eq!(
        door.piece(6, Given::attempt(1), &mut bufs).outcome,
        Some(Outcome::Ready)
    );
}

/// RED (audit plane-decisions, the request target): the kernel matches an exact claim on the
/// request PATH and hands `arrive` the whole target, query included; a claimed request with a
/// query routes as its path does, never the unclaimed `404` (THE DESIGN section 2: "a request routes
/// by its model; one configured model and none named is the default").
#[test]
fn a_claimed_request_with_a_query_routes_as_its_path() {
    let one = open(ONE);
    let queried = one.arrive_on(1, "POST", "/v1/systemone?trace=1", br#"{"state":{}}"#, &[]);
    assert_eq!(
        (queried.outcome, queried.route, queried.entry.as_str()),
        (Outcome::Ready, ROUTE_DIRECT, "jev"),
        "{queried:?}"
    );
}

/// RED (audit plane-decisions#6, the reclaimed unit): a unit whose arrival state the door no
/// longer holds is refused at its first piece, never served from a blank state that drops its
/// `upstream_model` splice and its relayed caller fields (THE DESIGN section 2: "`upstream_model`
/// rewrites only the top-level model value ... when it is set and differs").
#[test]
fn a_unit_whose_arrival_state_is_gone_is_refused_never_served_blank() {
    let upstream = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"}}}"#;
    let door = open(upstream);
    let body = br#"{"model":"jev","state":{}}"#;
    assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);
    // The host ends the unit before its first piece (a cancel naming it): its state goes.
    door.cancel_unit(1);
    let mut bufs = Bufs::new(1 << 16);
    let attempt = door.piece(1, Given::attempt(1), &mut bufs);
    let caller = door.piece(1, Given::caller(body), &mut bufs);
    assert!(
        !(caller.far && caller.bytes == body),
        "unit 1 reached the far end with its model unspliced: {attempt:?} {caller:?}"
    );
    assert_eq!(attempt.outcome, Some(Outcome::Refused), "{attempt:?}");
}

/// RED (audit plane-decisions R3, the refused reload): each generation's routes ride beside its
/// snapshot. A reload the host refuses after this plane's `refresh` (its seal fails) is retired by
/// the host, and arrivals route over the generation it kept serving again: a model only the
/// refused section names is unknown, and a model only it removed is routed (THE DESIGN section 6:
/// a refused reload leaves the running generation serving).
#[test]
fn a_reload_the_host_refused_leaves_the_running_generation_routing() {
    let door = open(r#"{"models":{"kept":{"provider":"typesafe"}}}"#);
    door.refresh(r#"{"models":{"added":{"provider":"typesafe"}}}"#, 2);
    assert_eq!(
        door.arrive(1, br#"{"model":"added"}"#).entry,
        "added",
        "a sealed generation routes over its own models"
    );
    // The host could not seal generation 2: it retires it and keeps serving generation 1.
    assert_eq!(door.retire(2), Outcome::Ready);
    let kept = door.arrive(2, br#"{"model":"kept"}"#);
    assert_eq!(
        (kept.outcome, kept.entry.as_str()),
        (Outcome::Ready, "kept"),
        "the running generation's model: {kept:?}"
    );
    let added = door.arrive(3, br#"{"model":"added"}"#);
    assert_eq!(
        (added.outcome, added.status),
        (Outcome::Refused, 404),
        "the refused generation's model: {added:?}"
    );
    // The next reload the host keeps routes over its own models, and retiring the one before it
    // changes nothing.
    door.refresh(r#"{"models":{"next":{"provider":"typesafe"}}}"#, 3);
    assert_eq!(door.retire(1), Outcome::Ready);
    assert_eq!(door.arrive(4, br#"{"model":"next"}"#).entry, "next");
}

/// RED (audit plane-decisions R3, the admission estimate): a READY arrival states what a success
/// bills at least, one decision and its request fee, ESTIMATED, so a budget in estimate mode prices
/// a decisions unit (THE DESIGN section 7: `admission: estimate` prices the plane's expected units).
#[test]
fn an_arrival_states_its_admission_estimate() {
    let door = open(ONE);
    let fee = class(busbar_contract::plane::PER_REQUEST);
    let arrived = door.arrive(1, br#"{"state":{}}"#);
    assert_eq!(arrived.outcome, Outcome::Ready, "{arrived:?}");
    let estimated = busbar_contract::abi::plane::UNITS_ESTIMATED;
    assert_eq!(
        arrived.expected,
        vec![(class("decision"), estimated, 1), (fee, estimated, 1)]
    );
    // A refused arrival states none.
    let refused = door.arrive(2, br#"{"model":"zz"}"#);
    assert!(refused.expected.is_empty(), "{refused:?}");
}

/// RED (audit plane-decisions R3, the unbounded echo): a requested model name longer than the
/// refusal's words may run is cut in the unknown-model refusal, so `arrive` answers its own 404 in
/// its dialect and never a FAULT past `MAX_REFUSAL_TEXT`; a name within the bound, however long,
/// is echoed whole, in 1.5.5's and predev's words.
#[test]
fn an_unknown_model_name_of_any_length_is_refused_in_bounded_words() {
    let two = open(TWO);
    let long = "k".repeat(8 * 1024);
    let whole = two.arrive(2, format!("{{\"model\":\"{long}\"}}").as_bytes());
    assert_eq!(
        (whole.outcome, whole.status, whole.text),
        (
            Outcome::Refused,
            404,
            format!("The model '{long}' does not exist or you do not have access to it.")
        ),
        "a name over a kilobyte is echoed whole"
    );
    let name = "x".repeat(600 * 1024);
    let body = format!("{{\"model\":\"{name}\"}}");
    let unknown = two.arrive(1, body.as_bytes());
    assert_eq!(
        (unknown.outcome, unknown.status),
        (Outcome::Refused, 404),
        "{}",
        unknown.text.len()
    );
    assert!(
        (unknown.text.len() as u64) <= busbar_contract::abi::plane::MAX_REFUSAL_TEXT,
        "the refusal's words run {} bytes",
        unknown.text.len()
    );
    assert!(
        unknown
            .text
            .ends_with("...' does not exist or you do not have access to it."),
        "only a name past the cap is cut, and marked cut"
    );
    assert!(
        unknown.text.starts_with("The model 'xxx"),
        "{}",
        &unknown.text[..32]
    );
}

/// A member nested `depth` levels deep: `[[...]]`.
fn nested(depth: usize) -> String {
    format!("{}{}", "[".repeat(depth), "]".repeat(depth))
}

/// RED (review of the routing, 1.5.5 parity): a body with a member nested past the span scanner's
/// depth (64; 1.5.5 read bodies to 128) still routes by the `model` it names. The scanner could not
/// walk it and the model was taken to be absent: several models configured, the request was
/// refused 400 as naming none.
#[test]
fn a_body_nested_past_the_span_scanners_depth_routes_by_its_model() {
    let two = open(TWO);
    let body = format!(r#"{{"model":"b","deep":{},"state":{{}}}}"#, nested(70));
    let routed = two.arrive(1, body.as_bytes());
    assert_eq!(
        (routed.outcome, routed.route, routed.entry.as_str()),
        (Outcome::Ready, ROUTE_DIRECT, "b"),
        "{routed:?}"
    );
}

/// RED (review of the routing, 1.5.5 parity): the same deep body with one model configured honours
/// the model it names (another is unknown), and the `upstream_model` splice applies, the model set
/// in the document as 1.5.5 read it and every other member keeping its value.
#[test]
fn a_deep_body_with_one_model_honours_its_model_and_is_spliced() {
    let upstream = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"}}}"#;
    let door = open(upstream);
    let other = format!(r#"{{"model":"other","deep":{}}}"#, nested(70));
    assert_eq!(
        door.arrive(2, other.as_bytes()).status,
        404,
        "another model is unknown"
    );
    let body = format!(
        r#"{{"model":"jev","deep":{},"state":{{"amount":7}}}}"#,
        nested(70)
    );
    assert_eq!(door.arrive(1, body.as_bytes()).entry, "jev");
    let mut bufs = Bufs::new(1 << 16);
    door.piece(1, Given::attempt(1), &mut bufs);
    let sent = door.piece(1, Given::caller(body.as_bytes()), &mut bufs);
    assert!(sent.far, "{sent:?}");
    let sent: serde_json::Value = serde_json::from_slice(&sent.bytes).expect("JSON");
    let mut want: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    want["model"] = serde_json::Value::String("jev-1.13.0".to_string());
    assert_eq!(sent, want);
}

/// RED (review of the routing, 1.5.5 parity): a body nested past 1.5.5's own parse depth (128) is
/// refused exactly as 1.5.5 refused a body it could not parse (v1.5.5 ingress/dispatch.rs:116):
/// 400, "We could not parse the JSON body of your request.", in the dialect's error shape.
#[test]
fn a_body_past_the_parse_depth_is_refused_as_1_5_5_refused_it() {
    let two = open(TWO);
    let body = format!(r#"{{"model":"a","deep":{}}}"#, nested(129));
    let refused = two.arrive(1, body.as_bytes());
    assert_eq!(
        (refused.outcome, refused.status, refused.text.as_str()),
        (
            Outcome::Refused,
            400,
            "We could not parse the JSON body of your request."
        ),
        "{refused:?}"
    );
    let (outcome, rendered) = two.refusal(1, refused.status, &refused.text, true);
    assert_eq!(outcome, Outcome::Ready);
    assert_eq!(
        rendered,
        br#"{"error":{"code":"invalid_request","message":"We could not parse the JSON body of your request."}}"#
    );
    // At 1.5.5's depth exactly, the body is read.
    let at = format!(r#"{{"model":"a","deep":{}}}"#, nested(127));
    assert_eq!(two.arrive(2, at.as_bytes()).entry, "a");
}

/// RED (review of the routing, 1.5.5 parity): a body the span scanner walks can still be no JSON
/// document, and 1.5.5 refused every such body before routing (v1.5.5 ingress/dispatch.rs:101-116,
/// `LazyBody::parse`, a full validating parse): trailing bytes, a bad number, a bad escape, not
/// JSON at all, a truncated body, and a deep body whose `model` is not a string (that one the
/// `model` refusal). Each was routed.
#[test]
fn a_body_1_5_5_could_not_parse_is_refused_before_routing() {
    let two = open(TWO);
    for (unit, body) in [
        (1, r#"{"model":"a"} garbage"#.to_string()),
        (2, r#"{"model":"a","n":01}"#.to_string()),
        (3, r#"{"model":"a","x":"\q"}"#.to_string()),
        (4, "not json".to_string()),
        (5, r#"{"model":"a","state":{"#.to_string()),
    ] {
        let refused = two.arrive(unit, body.as_bytes());
        assert_eq!(
            (refused.outcome, refused.status, refused.text.as_str()),
            (
                Outcome::Refused,
                400,
                "We could not parse the JSON body of your request."
            ),
            "{body}: {refused:?}"
        );
    }
    let one = open(ONE);
    let not_json = one.arrive(6, b"not json");
    assert_eq!(
        (not_json.outcome, not_json.status),
        (Outcome::Refused, 400),
        "one model configured routed a body that is not JSON: {not_json:?}"
    );
    let deep = format!(r#"{{"model":7,"deep":{}}}"#, nested(70));
    let not_a_name = two.arrive(7, deep.as_bytes());
    assert_eq!(
        (not_a_name.outcome, not_a_name.status),
        (Outcome::Refused, 400),
        "{not_a_name:?}"
    );
    assert_ne!(
        not_a_name.text, "We could not parse the JSON body of your request.",
        "a document 1.5.5 read, with a model that is not a name"
    );
}

/// The exact edge of 1.5.5's parse depth (128, v1.5.5 json.rs `MAX_JSON_DEPTH`): a body 128 deep is
/// read, 129 and 130 are refused.
#[test]
fn the_parse_depth_edge_is_1_5_5s() {
    let two = open(TWO);
    // `{"model":"a","deep":` is one level; `nested(n)` adds n.
    let at = |n: usize| format!(r#"{{"model":"a","deep":{}}}"#, nested(n));
    assert_eq!(two.arrive(1, at(127).as_bytes()).entry, "a", "128 deep");
    for (unit, n) in [(2, 128), (3, 129)] {
        let refused = two.arrive(unit, at(n).as_bytes());
        assert_eq!(
            (refused.outcome, refused.status),
            (Outcome::Refused, 400),
            "{} deep",
            n + 1
        );
    }
}

/// RED (1.5.5 parity): a request whose `model` is the empty string is refused 400 "Missing
/// required parameter: 'model'." (v1.5.5 ingress/dispatch.rs:138-152: an empty model is none), not
/// routed to the only model nor refused as an unknown one.
#[test]
fn an_empty_model_is_refused_as_1_5_5_refused_it() {
    for settings in [ONE, TWO] {
        let door = open(settings);
        let refused = door.arrive(1, br#"{"model":"","state":{}}"#);
        assert_eq!(
            (refused.outcome, refused.status, refused.text.as_str()),
            (
                Outcome::Refused,
                400,
                "Missing required parameter: 'model'."
            ),
            "{settings}: {refused:?}"
        );
    }
}

/// RED (review of the routing, 1.5.5 parity): 1.5.5 validated the whole body as UTF-8 before it
/// parsed it (sonic-rs `from_slice`) and refused invalid UTF-8 inside a string, or a stray byte past
/// the document, with its parse 400. Both routed.
#[test]
fn a_body_that_is_not_utf8_is_refused_as_1_5_5_refused_it() {
    let two = open(TWO);
    let in_string: &[u8] = b"{\"model\":\"a\",\"x\":\"\xff\"}";
    let trailing: &[u8] = b"{\"model\":\"a\"}\xc3";
    for (unit, body) in [(1, in_string), (2, trailing)] {
        let refused = two.arrive(unit, body);
        assert_eq!(
            (refused.outcome, refused.status, refused.text.as_str()),
            (
                Outcome::Refused,
                400,
                "We could not parse the JSON body of your request."
            ),
            "{body:?}: {refused:?}"
        );
    }
}

/// RED (1.5.5 parity): a `model` that is not a string was no model to 1.5.5, refused 400 "Missing
/// required parameter: 'model'." (v1.5.5 ingress/dispatch.rs:130-152).
#[test]
fn a_model_that_is_not_a_string_is_refused_as_1_5_5_refused_it() {
    let two = open(TWO);
    for (unit, body) in [(1, &br#"{"model":7}"#[..]), (2, br#"{"model":{"a":1}}"#)] {
        let refused = two.arrive(unit, body);
        assert_eq!(
            (refused.outcome, refused.status, refused.text.as_str()),
            (
                Outcome::Refused,
                400,
                "Missing required parameter: 'model'."
            ),
            "{refused:?}"
        );
    }
}

/// RED (1.5.5 parity): a lone surrogate escape in a top-level key, or in the `model` value, is no
/// string 1.5.5's head projection could decode: its parse 400. Both routed.
#[test]
fn a_lone_surrogate_in_a_top_level_key_or_the_model_is_refused() {
    let two = open(TWO);
    for (unit, body) in [
        (1, &br#"{"model":"a","\ud800":1}"#[..]),
        (2, br#"{"model":"a\ud800"}"#),
        // The values 1.5.5's head decoded (lazy_body.rs:41-52): `system`, `stream_options`, and a
        // `model` that is no string.
        (3, br#"{"model":"a","system":"\ud800"}"#),
        (4, br#"{"model":"a","stream_options":{"\ud800":1}}"#),
        (5, br#"{"model":{"\ud800":1}}"#),
    ] {
        let refused = two.arrive(unit, body);
        assert_eq!(
            (refused.outcome, refused.status, refused.text.as_str()),
            (
                Outcome::Refused,
                400,
                "We could not parse the JSON body of your request."
            ),
            "{refused:?}"
        );
    }
}

/// RED (1.5.5 parity): a member 1.5.5's head never decoded may carry a string its decoding would
/// refuse (a lone surrogate escape): 1.5.5 scanned past it and served the request. A body nested
/// past the span scanner's depth with such a member, its `model` first, routes by that model (the
/// whole document was decoded, and it was refused 400).
#[test]
fn a_member_1_5_5_never_decoded_does_not_refuse_the_body() {
    let two = open(TWO);
    let deep = format!(
        r#"{{"model":"b","deep":{}"\ud800"{}}}"#,
        "[".repeat(70),
        "]".repeat(70)
    );
    let routed = two.arrive(1, deep.as_bytes());
    assert_eq!(
        (routed.outcome, routed.entry.as_str()),
        (Outcome::Ready, "b"),
        "{routed:?}"
    );
    let shallow = two.arrive(2, br#"{"model":"a","x":"\ud800"}"#);
    assert_eq!(shallow.entry, "a", "{shallow:?}");
}

/// RED (1.5.5 parity): a request whose `model` must be rewritten to its route's `upstream_model`
/// but whose document does not parse (a member 1.5.5's head never decoded, which its hop's document
/// parse refused) was answered 500 "We received an unexpected internal error. Please try again."
/// (v1.5.5 proxy/engine/mod.rs:1499-1520): it is refused so, about its entry, never relayed with the
/// caller's model unspliced. One that needs no rewrite is served.
#[test]
fn a_request_that_cannot_take_its_upstream_model_is_refused_as_1_5_5_refused_it() {
    let upstream = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"}}}"#;
    let door = open(upstream);
    let refused = door.arrive(1, br#"{"model":"jev","x":"\ud800"}"#);
    assert_eq!(
        (
            refused.outcome,
            refused.status,
            refused.text.as_str(),
            refused.entry.as_str()
        ),
        (
            Outcome::Refused,
            500,
            "We received an unexpected internal error. Please try again.",
            "jev"
        ),
        "{refused:?}"
    );
    // Naming no model, there is nothing to rewrite.
    let unnamed = door.arrive(2, br#"{"x":"\ud800"}"#);
    assert_eq!(unnamed.outcome, Outcome::Ready, "{unnamed:?}");
}

/// RED (1.5.5 parity, sonic-rs 0.5.8's skip): an invalid `\u` hex escape inside a value 1.5.5 only
/// SKIPPED is accepted and the request is served; inside a value it DECODED (`system`, the `model`,
/// a key) it is the parse 400; and where the request's `model` must be spliced to an `upstream_model`
/// its DOM parse refused it, 500. The swallowed bytes of a skipped `\u` may hold the closing quote.
#[test]
fn an_invalid_hex_escape_in_a_skipped_value_is_served_as_1_5_5_served_it() {
    let two = open(TWO);
    let served = two.arrive(1, br#"{"model":"a","x":"\uZZZZ"}"#);
    assert_eq!(
        (served.outcome, served.entry.as_str()),
        (Outcome::Ready, "a"),
        "{served:?}"
    );
    let swallowed = two.arrive(2, br#"{"model":"a","x":"\u"   "}"#);
    assert_eq!(swallowed.outcome, Outcome::Ready, "{swallowed:?}");
    for (unit, body) in [
        (3, &br#"{"model":"a","system":"\uZZZZ"}"#[..]),
        (4, br#"{"model":"a","\uZZZZ":1}"#),
        (5, br#"{"model":"\uZZZZ"}"#),
    ] {
        let refused = two.arrive(unit, body);
        assert_eq!(
            (refused.outcome, refused.status, refused.text.as_str()),
            (
                Outcome::Refused,
                400,
                "We could not parse the JSON body of your request."
            ),
            "{refused:?}"
        );
    }
    let upstream = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"}}}"#;
    let refused = open(upstream).arrive(1, br#"{"model":"jev","x":"\uZZZZ"}"#);
    assert_eq!(
        (refused.outcome, refused.status, refused.text.as_str()),
        (
            Outcome::Refused,
            500,
            "We received an unexpected internal error. Please try again."
        ),
        "{refused:?}"
    );
}

/// RED (opus review, B2 and B3; 1.5.5 parity, v1.5.5 `lazy_body.rs` `Head` over sonic-rs 0.5.8): a
/// root number 1.5.5 decoded and found out of range (sonic-number 0.1.2 `FloatMustBeFinite`) is the
/// parse 400, where one in range is served over the one model; and a value nested past 128 behind a
/// `\u` escape that swallows its string's closing quote (which 1.5.5's depth scan reads as still
/// open) is served, as sonic's unbounded skip served it.
#[test]
fn a_root_number_out_of_range_is_refused_and_a_skipped_value_has_no_depth_bound() {
    let one = open(ONE);
    for (unit, body) in [
        (1, &b"1e400"[..]),
        (2, b"-1e309"),
        (3, b"1.7976931348623159e308"),
    ] {
        let refused = one.arrive(unit, body);
        assert_eq!(
            (refused.outcome, refused.status, refused.text.as_str()),
            (
                Outcome::Refused,
                400,
                "We could not parse the JSON body of your request."
            ),
            "{refused:?}"
        );
    }
    for (unit, body) in [
        (4, &b"1e308"[..]),
        (5, b"18446744073709551616"),
        (6, b"[1e400]"),
    ] {
        let served = one.arrive(unit, body);
        assert_eq!(
            (served.outcome, served.entry.as_str()),
            (Outcome::Ready, "jev"),
            "{served:?}"
        );
    }
    let two = open(TWO);
    let deep = format!(r#"{{"model":"a","x":"\uAAA\","y":{}}}"#, nested(200));
    let served = two.arrive(1, deep.as_bytes());
    assert_eq!(
        (served.outcome, served.entry.as_str()),
        (Outcome::Ready, "a"),
        "{served:?}"
    );
}

/// An `open` the door will not open answers FAILED with no instance, its reason in the buffer the
/// host lent (`err_len`): a section the grammar refuses, in the grammar's words, and a model whose
/// provider the host stated no protocol for, named with that provider.
#[test]
fn an_open_the_door_refuses_fails_with_its_reason() {
    assert_eq!(
        door_harness::open_refused(r#"{"modles":{}}"#, &[("typesafe", "jev")]),
        (
            Outcome::Failed,
            false,
            "unknown field `modles`, expected one of `models`, `hooks`, `upstream_credentials`"
                .to_string()
        )
    );
    assert_eq!(
        door_harness::open_refused(r#"{"models":{"m":{"provider":"elsewhere"}}}"#, &[]),
        (
            Outcome::Failed,
            false,
            "decisions.models.m names provider 'elsewhere', whose protocol the host did not state \
             when the plane opened, and states no `protocol` of its own; the decisions plane \
             speaks only: jev, nanojev."
                .to_string()
        )
    );
}
