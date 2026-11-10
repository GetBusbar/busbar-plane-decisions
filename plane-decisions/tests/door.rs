// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DOOR'S ANSWERS, DRIVEN OVER ITS ABI as the kernel's plane driver drives them.

#![allow(unsafe_code)]

mod door_harness;

use busbar_contract::abi::mechanism::call::Outcome;
use busbar_contract::abi::plane::{PRINCIPAL_REQUIRED, ROUTE_DIRECT};
use door_harness::{claims, open, open_with, Bufs, Given};

const SYSTEMONE: (&str, &str) = ("POST", "/v1/systemone");

const TWO: &str = r#"{"models":{"a":{"provider":"typesafe"},"b":{"provider":"typesafe"}}}"#;
const ONE: &str = r#"{"models":{"jev":{"provider":"typesafe"}}}"#;

#[test]
fn every_generation_with_a_model_claims_systemone() {
    assert_eq!(open(TWO).claims, claims(&[SYSTEMONE]), "two models");
    assert_eq!(open(ONE).claims, claims(&[SYSTEMONE]), "one model");
    let none = open(r#"{"models":{}}"#);
    assert!(none.claims.is_empty(), "no model claims nothing");

    assert_eq!(none.refresh(TWO, 2), claims(&[SYSTEMONE]));
    assert!(none.refresh("{}", 3).is_empty());
}

#[test]
fn a_request_routes_by_its_model() {
    let two = open(TWO);
    let b = two.arrive(1, br#"{"model":"b","state":{}}"#);
    assert_eq!(
        (b.outcome, b.route, b.entry.as_str()),
        (Outcome::Ready, ROUTE_DIRECT, "b"),
        "{b:?}"
    );

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

#[test]
fn upstream_model_is_spliced_into_the_top_level_model() {
    let upstream = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"}}}"#;
    let body = "{ \"state\" : {\"model\":\"inner\"},\n  \"model\" :  \"jev\" , \"n\":[1, 2] }";
    assert_eq!(
        sent(upstream, body.as_bytes()),
        "{ \"state\" : {\"model\":\"inner\"},\n  \"model\" :  \"jev-1.13.0\" , \"n\":[1, 2] }"
    );

    let same = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev"}}}"#;
    assert_eq!(sent(same, body.as_bytes()), body);
    assert_eq!(sent(ONE, body.as_bytes()), body);
    let unnamed = "{\"state\":{}}";
    assert_eq!(sent(upstream, unnamed.as_bytes()), unnamed);
}

/// One unit served over [`ONE`]: its ATTEMPT, the caller's body, and the far end's whole answer,
/// `status` with `answer`.
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

#[test]
fn a_success_with_no_count_bills_one_decision() {
    let fee = class(busbar_contract::plane::PER_REQUEST);
    let a = answered(200, br#"{"answers":{}}"#);
    assert_eq!(a.outcome, Some(Outcome::Ready), "{a:?}");
    assert_eq!(counts(&a), vec![(class("decision"), 1), (fee, 1)]);

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

#[test]
fn a_non_success_bills_nothing() {
    let a = answered(500, br#"{"usage":{"units":2}}"#);
    assert_eq!(a.outcome, Some(Outcome::Ready), "{a:?}");
    assert!(a.units.is_empty(), "{a:?}");
    assert_eq!(a.bytes, br#"{"usage":{"units":2}}"#, "relayed unchanged");
}

#[test]
fn a_success_with_an_unreadable_count_refuses_it_and_says_so() {
    let fee = class(busbar_contract::plane::PER_REQUEST);
    for answer in [
        &br#"{"usage":{"units":0.0000001}}"#[..],
        br#"{"usage":{"units":"7"}}"#,
        br#"{"usage":{"units":-3}}"#,
    ] {
        let a = answered(200, answer);
        let shown = String::from_utf8_lossy(answer);
        assert_eq!(
            a.verdict,
            busbar_contract::abi::plane::VERDICT_COUNT_REFUSED
        );
        assert_eq!(
            (a.outcome, a.short),
            (Some(Outcome::Ready), false),
            "{shown}: {a:?}"
        );
        assert_eq!(
            a.units,
            vec![(fee, busbar_contract::abi::plane::UNITS_REPORTED, 1),],
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

/// One unit served over `settings` on `target` in `dialect`: its far end answers 200 with `head`,
/// then its connection fails and the kernel hands `rest` as the answer's last piece, flagged
/// `PIECE_CUT`.
fn cut(
    settings: &str,
    target: &str,
    dialect: u32,
    head: &[u8],
    rest: &[u8],
) -> door_harness::Answered {
    let door = open_with(settings, &[("typesafe", "jev"), ("np", "nanojev")]);
    let body = br#"{"state":{}}"#;
    let arrived = door.arrive_on(1, "POST", target, body, &[]);
    assert_eq!(arrived.outcome, Outcome::Ready, "{arrived:?}");
    let mut bufs = Bufs::new(1 << 16);
    let mut attempt = Given::attempt(1);
    attempt.dialect = dialect;
    door.piece(1, attempt, &mut bufs);
    let mut caller = Given::caller(body);
    caller.dialect = dialect;
    assert!(door.piece(1, caller, &mut bufs).far);
    let mut first = Given::far(Some(200), false, head);
    first.dialect = dialect;
    assert_eq!(
        door.piece(1, first, &mut bufs).outcome,
        Some(Outcome::Ready)
    );
    let mut last = Given::far(None, true, rest);
    last.flags |= busbar_contract::abi::plane::PIECE_CUT;
    last.dialect = dialect;
    door.piece(1, last, &mut bufs)
}

#[test]
fn a_cut_answer_bills_only_the_counts_reported_before_the_cut() {
    let fee = class(busbar_contract::plane::PER_REQUEST);
    let a = cut(ONE, "/v1/systemone", 0, br#"{"answers":{"#, br#""x":"#);
    assert_eq!((a.outcome, a.done), (Some(Outcome::Ready), true), "{a:?}");
    assert!(
        a.units.is_empty(),
        "a cut answer bills nothing it did not report: {a:?}"
    );
    assert_eq!(a.bytes, br#""x":"#, "relayed unchanged");
    assert!(
        a.logs.is_empty(),
        "a cut is no unreadable usage: {:?}",
        a.logs
    );

    let a = cut(
        ONE,
        "/v1/systemone",
        0,
        br#"{"usage":{"units":3,"input_tokens":9},"#,
        br#""answers":{"#,
    );
    assert_eq!(
        a.units,
        vec![
            (
                class("decision"),
                busbar_contract::abi::plane::UNITS_REPORTED,
                3
            ),
            (
                class("input_tokens"),
                busbar_contract::abi::plane::UNITS_REPORTED,
                9
            ),
        ],
        "{a:?}"
    );
    assert!(
        !a.units.iter().any(|u| u.0 == fee),
        "no fee on a cut: {a:?}"
    );
    assert!(a.logs.is_empty(), "{:?}", a.logs);

    let a = cut(ONE, "/v1/systemone", 0, br#"{"usage":{"units":1"#, b"2");
    assert!(a.units.is_empty(), "{a:?}");
    assert!(a.logs.is_empty(), "{:?}", a.logs);

    let both = r#"{"models":{"nano":{"provider":"np","protocol":"nanojev"}}}"#;
    let a = cut(both, "/api/evaluate", 1, br#"{"result":{"#, b"}");
    assert!(a.units.is_empty(), "{a:?}");
    assert!(a.logs.is_empty(), "{:?}", a.logs);
}

#[test]
fn a_success_that_is_no_json_object_bills_the_floor_and_says_so() {
    let fee = class(busbar_contract::plane::PER_REQUEST);
    let floor = vec![
        (
            class("decision"),
            busbar_contract::abi::plane::UNITS_FLOOR,
            1,
        ),
        (fee, busbar_contract::abi::plane::UNITS_REPORTED, 1),
    ];
    for answer in [&b"<html>bad gateway</html>"[..], b"", b"  [1,2]", b"\"ok\""] {
        let a = answered(200, answer);
        let shown = String::from_utf8_lossy(answer);
        assert_eq!(a.outcome, Some(Outcome::Ready), "{shown}: {a:?}");
        assert_eq!(a.units, floor, "{shown}");
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
    let silent = answered(200, br#" {"answers":{}}"#);
    assert_eq!(silent.units, floor);
    assert!(silent.logs.is_empty(), "{:?}", silent.logs);

    let bom = answered(200, b"\xEF\xBB\xBF{\"usage\":{\"units\":3}}");
    assert_eq!(
        bom.units,
        vec![
            (
                class("decision"),
                busbar_contract::abi::plane::UNITS_REPORTED,
                3
            ),
            (fee, busbar_contract::abi::plane::UNITS_REPORTED, 1),
        ]
    );
    assert!(bom.logs.is_empty(), "{:?}", bom.logs);
}

#[test]
fn the_far_ends_trailers_are_neither_relayed_nor_read() {
    let door = open(ONE);
    let body = br#"{"state":{}}"#;
    assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);
    let mut bufs = Bufs::new(1 << 16);
    door.piece(1, Given::attempt(1), &mut bufs);
    assert!(door.piece(1, Given::caller(body), &mut bufs).far);
    let answer = br#"{"usage":{"units":2}}"#;
    let first = door.piece(1, Given::far(Some(200), false, answer), &mut bufs);
    assert_eq!(first.outcome, Some(Outcome::Ready), "{first:?}");
    let mut trailers = Given::far(None, true, b"{\"error\":1,\"usage\":{\"units\":9}}\r\n");
    trailers.flags |= busbar_contract::abi::plane::PIECE_FIELDS;
    let last = door.piece(1, trailers, &mut bufs);
    assert_eq!(
        (last.outcome, last.done),
        (Some(Outcome::Ready), true),
        "{last:?}"
    );
    let mut relayed = first.bytes.clone();
    relayed.extend_from_slice(&last.bytes);
    assert_eq!(
        String::from_utf8_lossy(&relayed),
        String::from_utf8_lossy(answer),
        "the caller's body is the far end's document alone"
    );
    assert_eq!(
        last.units,
        vec![
            (
                class("decision"),
                busbar_contract::abi::plane::UNITS_REPORTED,
                2
            ),
            (
                class(busbar_contract::plane::PER_REQUEST),
                busbar_contract::abi::plane::UNITS_REPORTED,
                1
            ),
        ],
        "{last:?}"
    );
}

#[test]
fn a_trailer_status_is_the_answers_status() {
    let door = open(ONE);
    let body = br#"{"state":{}}"#;
    assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);
    let mut bufs = Bufs::new(1 << 16);
    door.piece(1, Given::attempt(1), &mut bufs);
    assert!(door.piece(1, Given::caller(body), &mut bufs).far);
    let answer = br#"{"usage":{"units":2}}"#;
    let first = door.piece(1, Given::far(Some(200), false, answer), &mut bufs);
    assert_eq!(first.status, 200, "{first:?}");
    let mut trailers = Given::far(Some(500), true, b"grpc-status: 13\r\n");
    trailers.flags |= busbar_contract::abi::plane::PIECE_FIELDS;
    let last = door.piece(1, trailers, &mut bufs);
    assert_eq!(
        (last.outcome, last.done),
        (Some(Outcome::Ready), true),
        "{last:?}"
    );
    assert!(last.bytes.is_empty(), "{last:?}");
    assert!(
        last.units.is_empty(),
        "a failed answer bills nothing: {last:?}"
    );
}

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

#[test]
fn past_the_cap_an_arrival_is_refused_and_no_live_unit_dropped() {
    let door = open(ONE);
    let mut bufs = Bufs::new(1 << 16);
    for unit in 1..=MAX_UNITS {
        live(&door, unit, &mut bufs);
    }
    let over = door.arrive(MAX_UNITS + 1, br#"{"state":{}}"#);
    assert_eq!(
        (
            over.outcome,
            over.status,
            over.text.as_str(),
            over.route,
            over.entry.as_str()
        ),
        (
            Outcome::Refused,
            429,
            "busbar is holding as many decision calls as it admits at once; retry once one has ended",
            ROUTE_DIRECT,
            "jev"
        ),
        "{over:?}"
    );
    assert!(
        still_live(&door, 1, &mut bufs),
        "the first live unit was dropped"
    );
    assert!(still_live(&door, MAX_UNITS, &mut bufs));
}

#[test]
fn a_live_unit_not_yet_started_is_never_dropped_to_make_room() {
    let door = open(ONE);
    for unit in 1..=MAX_UNITS {
        assert_eq!(door.arrive(unit, br#"{}"#).outcome, Outcome::Ready);
    }
    let next = door.arrive(MAX_UNITS + 1, br#"{}"#);
    assert_eq!(
        (next.outcome, next.status),
        (Outcome::Refused, 429),
        "{next:?}"
    );
    let mut bufs = Bufs::new(1 << 16);
    let first = door.piece(1, Given::attempt(1), &mut bufs);
    assert_eq!(
        (first.outcome, first.request.as_str()),
        (Some(Outcome::Ready), "POST /v1/systemone"),
        "the first unit, which crossed no piece while the instance filled, was dropped: {first:?}"
    );

    assert_eq!(
        door.cancel_unit(2),
        (Outcome::Ready, busbar_contract::abi::plane::CANCEL_ABORTED)
    );
    assert_eq!(door.arrive(MAX_UNITS + 2, br#"{}"#).outcome, Outcome::Ready);
}

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

#[test]
fn a_unit_whose_arrival_state_is_gone_is_refused_never_served_blank() {
    let upstream = r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-1.13.0"}}}"#;
    let door = open(upstream);
    let body = br#"{"model":"jev","state":{}}"#;
    assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);

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

#[test]
fn a_reload_the_host_refused_leaves_the_running_generation_routing() {
    let door = open(r#"{"models":{"kept":{"provider":"typesafe"}}}"#);
    door.refresh(r#"{"models":{"added":{"provider":"typesafe"}}}"#, 2);
    assert_eq!(
        door.arrive(1, br#"{"model":"added"}"#).entry,
        "added",
        "a sealed generation routes over its own models"
    );

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

    door.refresh(r#"{"models":{"next":{"provider":"typesafe"}}}"#, 3);
    assert_eq!(door.retire(1), Outcome::Ready);
    assert_eq!(door.arrive(4, br#"{"model":"next"}"#).entry, "next");
}

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

    let refused = door.arrive(2, br#"{"model":"zz"}"#);
    assert!(refused.expected.is_empty(), "{refused:?}");
}

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

#[test]
fn a_body_past_the_parse_depth_is_refused() {
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

    let at = format!(r#"{{"model":"a","deep":{}}}"#, nested(127));
    assert_eq!(two.arrive(2, at.as_bytes()).entry, "a");
}

#[test]
fn malformed_json_is_refused_before_routing() {
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
        "a valid document with a model that is not a name"
    );
}

/// The bounded parse depth (128): a body 128 deep is
/// read, 129 and 130 are refused.
#[test]
fn the_parse_depth_edge_is_bounded() {
    let two = open(TWO);

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

#[test]
fn an_empty_model_is_refused() {
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

#[test]
fn a_body_that_is_not_utf8_is_refused() {
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

#[test]
fn a_model_that_is_not_a_string_is_refused() {
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

#[test]
fn a_lone_surrogate_in_a_top_level_key_or_the_model_is_refused() {
    let two = open(TWO);
    for (unit, body) in [
        (1, &br#"{"model":"a","\ud800":1}"#[..]),
        (2, br#"{"model":"a\ud800"}"#),
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

#[test]
fn invalid_unicode_is_refused_before_routing_with_or_without_an_alias() {
    for settings in [
        TWO,
        r#"{"models":{"a":{"provider":"typesafe","upstream_model":"upstream"}}}"#,
    ] {
        let door = open(settings);
        for (unit, body) in [
            (1, br#"{"model":"a","x":"\uZZZZ"}"#.as_slice()),
            (2, br#"{"model":"a","x":"\ud800"}"#),
            (3, br#"{"model":"a","system":"\uZZZZ"}"#),
            (4, br#"{"model":"a","x":"\u"   "}"#),
            (5, br#"{"x":"\ud800"}"#),
        ] {
            let refused = door.arrive(unit, body);
            assert_eq!(
                (refused.outcome, refused.status),
                (Outcome::Refused, 400),
                "{refused:?}"
            );
        }
    }
}

#[test]
fn opaque_numbers_route_and_alias_without_changing_other_bytes() {
    let settings = r#"{"models":{"a":{"provider":"typesafe","upstream_model":"upstream"}}}"#;
    let body = br#" { "model" : "a", "system":1e309, "stream":-1e400, "state":{"n":18446744073709551617,"z":-0,"v":27.0,"s":"\u0061"} } "#;
    let expected = String::from_utf8(body.to_vec())
        .unwrap()
        .replacen(r#""a""#, r#""upstream""#, 1);
    assert_eq!(sent(settings, body), expected);
    assert_eq!(sent(TWO, body), String::from_utf8(body.to_vec()).unwrap());
}

#[test]
fn opaque_root_numbers_keep_the_single_model_default() {
    let one = open(ONE);
    for (i, body) in ["1e400", "-1e309", "18446744073709551617"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            one.arrive(i as u64 + 1, body.as_bytes()).outcome,
            Outcome::Ready
        );
    }
}

#[test]
fn an_open_the_door_refuses_is_refused_with_its_reason() {
    assert_eq!(
        door_harness::open_refused(r#"{"modles":{}}"#, &[("typesafe", "jev")]),
        (
            Outcome::Refused,
            false,
            "unknown field `modles`, expected one of `models`, `hooks`, `upstream_credentials`"
                .to_string()
        )
    );
    assert_eq!(
        door_harness::open_refused(r#"{"models":{"m":{"provider":"elsewhere"}}}"#, &[]),
        (
            Outcome::Refused,
            false,
            "decisions.models.m names provider 'elsewhere', whose protocol the host did not state \
             when the plane opened, and states no `protocol` of its own; the decisions plane \
             speaks only: jev, nanojev."
                .to_string()
        )
    );
}

/// The provider's final usage parent is the only one the served door bills. An absent optional
/// jev count is a silent floor; an absent required nanojev count is a floor with one warning.
#[test]
fn repeated_usage_parents_bill_only_their_replacement_over_every_fragment_split() {
    let settings = r#"{"models":{"jev":{"provider":"typesafe"},"nano":{"provider":"np","protocol":"nanojev"}}}"#;
    type Case<'a> = (u32, &'a str, &'a [u8], u64, bool, usize);
    let cases: &[Case<'_>] = &[
        (
            0,
            "/v1/systemone",
            br#"{"usage":{"units":500,"input_tokens":90},"usage":{"units":2}}"#,
            2,
            false,
            0,
        ),
        (
            0,
            "/v1/systemone",
            br#"{"usage":{"units":500},"\u0075sage":{}}"#,
            1,
            true,
            0,
        ),
        (
            0,
            "/v1/systemone",
            br#"{"usage":{"units":500},"usage":null}"#,
            1,
            true,
            0,
        ),
        (
            0,
            "/v1/systemone",
            br#"{"usage":{"units":500},"usage":7}"#,
            1,
            true,
            0,
        ),
        (
            0,
            "/v1/systemone",
            br#"{"usage":{"units":"bad"},"usage":{}}"#,
            1,
            true,
            0,
        ),
        (
            1,
            "/api/evaluate",
            br#"{"execution":{"states":500},"\u0065xecution":{}}"#,
            1,
            true,
            1,
        ),
        (
            1,
            "/api/evaluate",
            br#"{"execution":{"states":500},"execution":null}"#,
            1,
            true,
            1,
        ),
        (
            1,
            "/api/evaluate",
            br#"{"execution":{"states":500},"execution":7}"#,
            1,
            true,
            1,
        ),
    ];
    for &(dialect, target, answer, amount, floor, warnings) in cases {
        for at in 0..=answer.len() {
            let door = open_with(settings, &[("typesafe", "jev"), ("np", "nanojev")]);
            let body = br#"{"state":{}}"#;
            assert_eq!(
                door.arrive_on(1, "POST", target, body, &[]).outcome,
                Outcome::Ready
            );
            let mut bufs = Bufs::new(1 << 16);
            let mut attempt = Given::attempt(1);
            attempt.dialect = dialect;
            door.piece(1, attempt, &mut bufs);
            let mut caller = Given::caller(body);
            caller.dialect = dialect;
            door.piece(1, caller, &mut bufs);
            let mut head = Given::far(Some(200), false, &answer[..at]);
            head.dialect = dialect;
            let first = door.piece(1, head, &mut bufs);
            assert!(first.units.is_empty() && first.logs.is_empty(), "{first:?}");
            let mut tail = Given::far(None, true, &answer[at..]);
            tail.dialect = dialect;
            let last = door.piece(1, tail, &mut bufs);
            assert_eq!(
                (last.outcome, last.done),
                (Some(Outcome::Ready), true),
                "{last:?}"
            );
            let source = if floor {
                busbar_contract::abi::plane::UNITS_FLOOR
            } else {
                busbar_contract::abi::plane::UNITS_REPORTED
            };
            assert_eq!(
                last.units,
                vec![
                    (class("decision"), source, amount),
                    (
                        class(busbar_contract::plane::PER_REQUEST),
                        busbar_contract::abi::plane::UNITS_REPORTED,
                        1
                    ),
                ],
                "{answer:?} split at {at}"
            );
            assert_eq!(
                last.logs
                    .iter()
                    .filter(|l| l.contains("usage is unreadable"))
                    .count(),
                warnings,
                "{last:?}"
            );
            let mut relayed = first.bytes;
            relayed.extend_from_slice(&last.bytes);
            assert_eq!(relayed, answer, "{answer:?} split at {at}");
        }
    }
}

#[test]
fn an_unreadable_usage_warns_once_after_a_short_answer_fits() {
    let door = open(ONE);
    let mut bufs = Bufs::new(1 << 16);
    live(&door, 1, &mut bufs);
    assert!(still_live(&door, 1, &mut bufs));
    let answer = br#"{"usage":{"units":"bad"}}"#;
    bufs.set_units_cap(0);
    let short = door.piece(1, Given::whole(200, answer), &mut bufs);
    assert_eq!(short.outcome, Some(Outcome::Failed), "{short:?}");
    assert_eq!(short.tap_fault, 0, "{short:?}");
    assert!(
        short.short && short.bytes.is_empty() && short.logs.is_empty(),
        "{short:?}"
    );
    bufs.set_units_cap(8);
    let recalled = door.piece(1, Given::whole(200, answer), &mut bufs);
    assert_eq!(
        (recalled.outcome, recalled.done),
        (Some(Outcome::Ready), true),
        "{recalled:?}"
    );
    assert_eq!(recalled.bytes, answer);
    assert_eq!(
        recalled.tap_fault,
        busbar_contract::abi::plane::TAP_FAULT_DECODE,
        "{recalled:?}"
    );
    assert_eq!(
        recalled.verdict,
        busbar_contract::abi::plane::VERDICT_COUNT_REFUSED,
        "{recalled:?}"
    );
    assert_eq!(
        counts(&recalled),
        vec![(class(busbar_contract::plane::PER_REQUEST), 1)]
    );
    assert_eq!(
        recalled
            .logs
            .iter()
            .filter(|l| l.contains("usage is unreadable"))
            .count(),
        1,
        "{recalled:?}"
    );
}

/// 5029 is benign-recurring (warn-once, latched): the same unreadable member on a second answer of
/// the same instance bills only the request fee, refuses the unreadable count, and logs at debug
/// instead of issuing a second warning.
#[test]
fn an_unreadable_usage_warns_once_per_instance_not_per_answer() {
    let door = open(ONE);
    let answer = br#"{"usage":{"units":"bad"}}"#;
    let mut warnings = Vec::new();
    for unit in 1..=3 {
        let mut bufs = Bufs::new(1 << 16);
        live(&door, unit, &mut bufs);
        assert!(still_live(&door, unit, &mut bufs));
        let a = door.piece(unit, Given::whole(200, answer), &mut bufs);
        assert_eq!((a.outcome, a.done), (Some(Outcome::Ready), true), "{a:?}");
        assert_eq!(
            counts(&a),
            vec![(class(busbar_contract::plane::PER_REQUEST), 1)],
            "unit {unit}"
        );
        warnings.push(
            a.logs
                .iter()
                .filter(|l| l.contains("usage is unreadable"))
                .count(),
        );
    }
    assert_eq!(warnings, vec![1, 0, 0]);
}

/// Direct projection of malformed input returns a marked fault; the shared core must fail
/// closed for content gates instead of substituting an empty request.
#[test]
fn project_over_an_unparseable_body_marks_its_fault_unparseable() {
    let door = open(r#"{"models":{"jev":{"provider":"typesafe"}}}"#);
    let body = br#"{"model":"jev","state":"\ud800"}"#;
    // Ingress rejects this malformed body before retaining a unit. A direct projection must
    // still report the parse fault, using the current generation when no unit was retained.
    let refused = door.arrive(1, body);
    assert_eq!(
        (refused.outcome, refused.status),
        (Outcome::Refused, 400),
        "{refused:?}"
    );
    let got = door.project(1, "/v1/systemone", body, None);
    assert_eq!(got.outcome, Some(Outcome::Fault), "{got:?}");
    assert!(got.fault_unparseable, "{got:?}");
}

/// The ATTEMPT piece writes the JSON content-type, then the caller's relayed fields in arrival
/// order, and never a credential carrier.
#[test]
fn the_attempt_carries_content_type_then_the_callers_relayed_fields() {
    let door = open(ONE);
    let arrived = door.arrive_on(
        1,
        "POST",
        "/v1/systemone",
        br#"{"state":{}}"#,
        &[
            ("X-Trace", "t-1"),
            ("Authorization", "Bearer k"),
            ("accept-language", "en"),
        ],
    );
    assert_eq!(arrived.outcome, Outcome::Ready, "{arrived:?}");
    let mut bufs = Bufs::new(1 << 16);
    let attempt = door.piece(1, Given::attempt(1), &mut bufs);
    let expected: Vec<(String, String)> = [
        ("content-type", "application/json"),
        ("x-trace", "t-1"),
        ("accept-language", "en"),
    ]
    .iter()
    .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
    .collect();
    assert_eq!(attempt.fields, expected);
}

#[test]
fn exact_fractional_usage_reaches_the_live_unit_count_abi() {
    use busbar_contract::abi::plane::{UNITS_REPORTED, UNITS_REPORTED_MICROS};
    for (raw, source, amount) in [
        ("27.5", UNITS_REPORTED_MICROS, 27_500_000),
        ("27.1", UNITS_REPORTED_MICROS, 27_100_000),
        ("27.0", UNITS_REPORTED, 27),
        ("1e3", UNITS_REPORTED, 1000),
        ("0", UNITS_REPORTED, 0),
        (
            "9223372036854.775807",
            UNITS_REPORTED_MICROS,
            i64::MAX as u64,
        ),
    ] {
        let body = format!(
            r#"{{"usage":{{"units":{raw},"input_tokens":27.1,"output_tokens":0.000001}}}}"#
        );
        let answer = answered(200, body.as_bytes());
        assert_eq!(answer.bytes, body.as_bytes());
        assert_eq!(
            answer.units,
            vec![
                (class("decision"), source, amount),
                (class("input_tokens"), UNITS_REPORTED_MICROS, 27_100_000),
                (class("output_tokens"), UNITS_REPORTED_MICROS, 1),
                (
                    class(busbar_contract::plane::PER_REQUEST),
                    UNITS_REPORTED,
                    1
                ),
            ],
            "{raw}"
        );
    }
}

/// The kernel's wait reaches the caller as `Retry-After` beside the refusal; with no wait there is
/// no such field.
#[test]
fn a_refusal_with_a_wait_states_it_as_retry_after() {
    let door = open(ONE);
    let (outcome, body, stated) = door.refusal_waiting(0, 429, "slow down", false, 17);
    assert_eq!(outcome, Outcome::Ready);
    assert_eq!(
        body,
        br#"{"error":{"code":"rate_limit","message":"slow down"}}"#
    );
    assert!(stated.iter().any(|f| f == "retry-after: 17"), "{stated:?}");
    let (_, _, none) = door.refusal_waiting(0, 429, "slow down", false, 0);
    assert!(
        !none.iter().any(|f| f.starts_with("retry-after")),
        "{none:?}"
    );
}
