// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE HOOK VIEW, DRIVEN OVER THE DOOR'S ABI as the kernel's hook stage drives it (THE DESIGN
//! section 1: "Every plane gets every capability ... The breaker, the hook tap and gate ... apply
//! to every plane the same way"). With a hook bound, the kernel asks `project` for the unit's
//! request view before its gates and its rewrites run, and hands a request-stage rewrite back to
//! `project`, which applies it in the request's dialect and answers the body the kernel keeps and
//! re-pushes. With none bound, `project` is never asked and the caller's bytes reach the far end as
//! they came (`tests/door.rs`).

#![allow(unsafe_code)]

mod door_harness;

use busbar_contract::abi::mechanism::call::Outcome;
use door_harness::{open, open_with, Bufs, Given};

const SYSTEMONE: &str = "/v1/systemone";
const TWO: &str = r#"{"models":{"a":{"provider":"typesafe"},"b":{"provider":"typesafe"}}}"#;

/// A hook's rewrite (1.5.5's rewrite JSON, as the kernel hands it): one turn whose content is the
/// request document `document`.
fn rewrite_of(document: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "messages": [{ "role": "user", "content": document }],
        "tools": [],
    }))
    .expect("JSON")
}

/// RED (audit plane-decisions R3, the hook capability): `project` answers the unit's request view
/// (it was REFUSED, so any bound gate, rewrite or route policy refused every decisions unit): the
/// entry it routes over as the view's pool, its dialect, one turn carrying the request document,
/// and the document as the projected body, every byte the caller's.
#[test]
fn a_decisions_request_is_projected_for_the_hooks() {
    let door = open(TWO);
    let body = br#"{"model":"b","state":{"amount":7}}"#;
    assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);
    let view = door.project(1, SYSTEMONE, body, None);
    assert_eq!(view.outcome, Some(Outcome::Ready), "{view:?}");
    assert_eq!(
        (view.pool.as_str(), view.dialect.as_str()),
        ("b", "jev"),
        "{view:?}"
    );
    let text = String::from_utf8(body.to_vec()).expect("UTF-8");
    assert_eq!(view.turns, vec![("user".to_string(), text.clone())]);
    assert_eq!(
        (view.message_count, view.total_chars),
        (1, text.chars().count() as u64)
    );
    assert_eq!(view.body.as_deref(), Some(&body[..]));
    assert_eq!(view.rewritten, None, "no rewrite was handed");
}

/// The second dialect's request is projected in its own dialect, over its own models.
#[test]
fn a_nanojev_request_is_projected_in_its_dialect() {
    let door = open_with(
        r#"{"models":{"n":{"provider":"np"},"j":{"provider":"typesafe"}}}"#,
        &[("np", "nanojev"), ("typesafe", "jev")],
    );
    let body = br#"{"input":{}}"#;
    let view = door.project(1, "/api/evaluate", body, None);
    assert_eq!(
        (view.outcome, view.pool.as_str(), view.dialect.as_str()),
        (Some(Outcome::Ready), "n", "nanojev"),
        "{view:?}"
    );
}

/// RED (audit plane-decisions R3, the hook capability): a request-stage hook's rewrite REPLACES the
/// request with its document, as 1.5.5 replaced the body: what the far end receives is the
/// hook's document (the caller's `model` kept), stated here on its own and never read off the
/// caller's request; the rewritten request is both answered as the body the kernel keeps and the
/// request projected, and the kernel re-pushes that body.
#[test]
fn a_rewrite_alters_the_request_the_far_end_receives() {
    let door = open(TWO);
    let body = b"{\"model\":\"a\",  \"state\":{\"amount\":7}, \"trace\":\"t-1\"}";
    assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);
    let view = door.project(
        1,
        SYSTEMONE,
        body,
        Some(&rewrite_of(r#"{"state":{"amount":0}}"#)),
    );
    // The hook's document, with the routed model: what replaces the request.
    let want: &[u8] = br#"{"model":"a","state":{"amount":0}}"#;
    assert_eq!(view.outcome, Some(Outcome::Ready), "{view:?}");
    assert_eq!(view.rewritten.as_deref(), Some(want), "{view:?}");
    assert_eq!(
        view.body.as_deref(),
        Some(want),
        "the rewritten request is projected"
    );
    // The kernel keeps the body `project` answered as rewritten and pushes it as the caller's
    // piece: the far end receives the rewrite, never the caller's original.
    let kept = view.rewritten.expect("a rewritten body is answered");
    let mut bufs = Bufs::new(1 << 16);
    door.piece(1, Given::attempt(1), &mut bufs);
    let sent = door.piece(1, Given::caller(&kept), &mut bufs);
    assert!(sent.far, "{sent:?}");
    assert_eq!(sent.bytes, want);
}

/// RED (coordinator ruling, 1.5.5 parity): a member the hook's document omits is dropped from the
/// request, so a hook redacts by omission.
#[test]
fn a_rewrite_redacts_by_omission() {
    let door = open(TWO);
    let body = br#"{"model":"a","state":{"amount":7},"pii":{"ssn":"123-45-6789"}}"#;
    let view = door.project(
        1,
        SYSTEMONE,
        body,
        Some(&rewrite_of(r#"{"state":{"amount":7}}"#)),
    );
    let want: &[u8] = br#"{"model":"a","state":{"amount":7}}"#;
    assert_eq!(view.rewritten.as_deref(), Some(want), "{view:?}");
}

/// RED (review of the rewrite, the span scanner's depth): a caller cannot make a hook's rewrite
/// silently not apply by nesting a member deeper than the span scanner reads (64): the field a
/// redaction hook omits never reaches the far end.
#[test]
fn a_redaction_applies_over_a_deeply_nested_body() {
    let door = open(TWO);
    let deep = format!("{}{}", "[".repeat(70), "]".repeat(70));
    let body =
        format!(r#"{{"model":"a","deep":{deep},"pii":{{"ssn":"123-45-6789"}},"state":{{}}}}"#);
    let view = door.project(
        1,
        SYSTEMONE,
        body.as_bytes(),
        Some(&rewrite_of(r#"{"state":{},"deep":[]}"#)),
    );
    assert_eq!(view.outcome, Some(Outcome::Ready), "{view:?}");
    let kept = view.rewritten.expect("the hook's rewrite applies");
    assert!(
        !String::from_utf8_lossy(&kept).contains("123-45-6789"),
        "the omitted field reaches the far end"
    );
    // The kernel pushes the body `project` answered as the unit's caller piece.
    let mut bufs = Bufs::new(1 << 16);
    assert_eq!(door.arrive(1, br#"{"model":"a"}"#).outcome, Outcome::Ready);
    door.piece(1, Given::attempt(1), &mut bufs);
    let sent = door.piece(1, Given::caller(&kept), &mut bufs);
    assert!(sent.far && !String::from_utf8_lossy(&sent.bytes).contains("123-45-6789"));
}

/// RED (coordinator ruling): a rewrite naming another model is no switch of provider: `project`
/// REFUSES it, the hook's failure, which the kernel answers by that hook's `on_error`; naming the
/// routed model applies.
#[test]
fn a_rewrite_naming_another_model_is_refused() {
    let door = open(TWO);
    let body = br#"{"model":"a","state":{}}"#;
    let switched = door.project(
        1,
        SYSTEMONE,
        body,
        Some(&rewrite_of(r#"{"model":"b","state":{}}"#)),
    );
    assert_eq!(switched.outcome, Some(Outcome::Refused), "{switched:?}");
    let same = door.project(
        1,
        SYSTEMONE,
        body,
        Some(&rewrite_of(r#"{"model":"a","state":{"x":1}}"#)),
    );
    assert_eq!(
        same.rewritten.as_deref(),
        Some(&br#"{"model":"a","state":{"x":1}}"#[..]),
        "{same:?}"
    );
}

/// A rewrite that does not apply leaves the request as it was (1.5.5): the view is the caller's
/// request, and nothing is answered as rewritten.
#[test]
fn a_rewrite_that_does_not_apply_leaves_the_request() {
    let door = open(TWO);
    let body = br#"{"model":"a","state":{}}"#;
    for rewrite in [
        &br#"{"messages":[],"tools":[]}"#[..],
        b"not json",
        &rewrite_of("no document")[..],
    ] {
        let view = door.project(1, SYSTEMONE, body, Some(rewrite));
        assert_eq!(view.outcome, Some(Outcome::Ready), "{view:?}");
        assert_eq!(view.rewritten, None, "{view:?}");
        assert_eq!(view.body.as_deref(), Some(&body[..]));
    }
}

/// A target no claim of this plane names carries none of its requests: it is not projected.
#[test]
fn a_target_the_plane_does_not_claim_is_not_projected() {
    let door = open(TWO);
    assert_eq!(
        door.project(1, "/v1/models", b"{}", None).outcome,
        Some(Outcome::Refused)
    );
}

/// RED (review of the rewrite, 1.5.5's depth): a hook's rewrite applies to a request exactly as
/// deep as 1.5.5 read (128): the request was read one level short of it, and the rewrite did not
/// apply, the field the hook omitted going upstream.
#[test]
fn a_redaction_applies_at_1_5_5s_parse_depth() {
    let door = open(TWO);
    // `{"deep":` is one level; 127 more make 128.
    let deep = format!("{}{}", "[".repeat(127), "]".repeat(127));
    let body = format!(r#"{{"model":"a","deep":{deep},"pii":"123-45-6789","state":{{}}}}"#);
    let view = door.project(
        1,
        SYSTEMONE,
        body.as_bytes(),
        Some(&rewrite_of(r#"{"state":{},"deep":[]}"#)),
    );
    assert_eq!(view.outcome, Some(Outcome::Ready), "{view:?}");
    let kept = view
        .rewritten
        .expect("the hook's rewrite applies at 128 deep");
    assert!(!String::from_utf8_lossy(&kept).contains("123-45-6789"));
}

/// A request whose document does not parse (a member 1.5.5's head never decoded, carrying a lone
/// surrogate escape or a bad `\u` escape) passes `arrive`, as 1.5.5's head scan passed it. A
/// request-stage hook's rewrite WITH a document over it is a FAULT: 1.5.5 materialized the
/// document for its rewrite pass, could not, and failed closed with 500 `request rewrite could not
/// be applied` (`proxy/engine/mod.rs:749-755`). The plane never applies the hook's document over a
/// body it cannot read and never answers "does not apply": its `project` answers FAULT, which the
/// kernel always answers 500 whatever the hook's `on_error` (RED kernel test
/// `a_faulted_rewrite_project_is_always_the_shared_500_under_every_on_error`), the body never
/// forwarded. This harness drives the plane alone, so what it shows is the FAULT and that no
/// rewritten body is answered.
#[test]
fn a_rewrite_over_a_body_whose_document_does_not_parse_is_a_fault() {
    let door = open(TWO);
    for body in [
        &br#"{"model":"a","pii":"123-45-6789","x":"\ud800"}"#[..],
        br#"{"model":"a","pii":"123-45-6789","x":"\uZZZZ"}"#,
    ] {
        assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);
        let view = door.project(1, SYSTEMONE, body, Some(&rewrite_of(r#"{"state":{}}"#)));
        assert_eq!(view.outcome, Some(Outcome::Fault), "{view:?}");
        assert_eq!(view.rewritten, None, "the hook's document is never applied");
    }
}

/// 1.5.5 materialized the request's document before any rewrite hook ran (`proxy/engine/mod.rs:728`,
/// `:749`): the kernel's first `project`, made with NO rewrite before any hook runs, is a FAULT for a
/// body whose document does not parse (a skipped member the head accepted), so the shared 500 comes
/// whether the hook would have abstained, failed or rejected. A body that parses projects as always.
#[test]
fn a_project_with_no_rewrite_over_a_body_whose_document_does_not_parse_is_a_fault() {
    let door = open(TWO);
    for body in [
        &br#"{"model":"a","x":"\ud800"}"#[..],
        br#"{"model":"a","x":"\uZZZZ"}"#,
    ] {
        assert_eq!(door.arrive(1, body).outcome, Outcome::Ready);
        let view = door.project(1, SYSTEMONE, body, None);
        assert_eq!(view.outcome, Some(Outcome::Fault), "{view:?}");
    }
    let ok = br#"{"model":"a","x":"\u00e9"}"#;
    assert_eq!(door.arrive(2, ok).outcome, Outcome::Ready);
    assert_eq!(
        door.project(2, SYSTEMONE, ok, None).outcome,
        Some(Outcome::Ready)
    );
}

/// RED (audit plane-decisions round cb0ed01e04, D40): `project` sees the unit as `arrive` routed
/// it, over the generation `arrive` chose: a reload landing between the two changes neither the
/// view's pool nor which model a rewrite may name. A unit `arrive` keeps no state for is projected
/// over the newest generation. RED before: `project` re-routed over the newest generation, so the
/// reloaded unit's pool was empty and a rewrite naming its own model was refused.
#[test]
fn a_unit_is_projected_over_the_generation_it_arrived_in() {
    let door = open(r#"{"models":{"jev":{"provider":"typesafe"}}}"#);
    let body = br#"{"state":{"amount":7}}"#;
    assert_eq!(door.arrive(1, body).entry, "jev");
    door.refresh(r#"{"models":{"other":{"provider":"typesafe"}}}"#, 2);
    let view = door.project(1, SYSTEMONE, body, None);
    assert_eq!(
        (view.outcome, view.pool.as_str()),
        (Some(Outcome::Ready), "jev"),
        "{view:?}"
    );
    let rewrite = rewrite_of(r#"{"model":"jev","state":{"amount":8}}"#);
    let rewritten = door.project(1, SYSTEMONE, body, Some(&rewrite));
    assert_eq!(
        (rewritten.outcome, rewritten.pool.as_str()),
        (Some(Outcome::Ready), "jev"),
        "{rewritten:?}"
    );
    assert_eq!(
        rewritten.rewritten.as_deref(),
        Some(&br#"{"state":{"amount":8}}"#[..])
    );
    // No state kept for the unit: the newest generation routes it.
    assert_eq!(door.project(9, SYSTEMONE, body, None).pool, "other");
}
