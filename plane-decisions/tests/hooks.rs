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
