// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE PII WITNESS, OVER THE SERVED DOOR. A decision request carries the caller's `state` and the
//! provider's answer carries its `answers`: the decision content. The door relays both, byte for
//! byte, and states nothing of either itself: not in an arrival's route or words, not in an
//! answer's head fields, counts or records, not in a refusal it renders. This drives the door over
//! a request and answers (a success, and an error answer that echoes the caller's state) whose
//! content carries markers, and fails on a marker in anything the door states beside the relayed
//! bytes themselves.

#![allow(unsafe_code)]

mod door_harness;

use busbar_contract::abi::mechanism::call::Outcome;
use door_harness::{open, Answered, Bufs, Given};

/// What the caller's `state` carries.
const STATE_MARKER: &str = "CALLER-SECRET-STATE-9f3a";
/// What the provider's `answers` carry.
const ANSWERS_MARKER: &str = "PROVIDER-ANSWER-CONTENT-77bd";

/// Whether `text` carries either marker.
fn leaks(text: &str) -> bool {
    text.contains(STATE_MARKER) || text.contains(ANSWERS_MARKER)
}

/// Everything an `on_piece` answer states beside the bytes it relays: its request line, its head
/// fields, its counts and its log lines.
fn stated(a: &Answered) -> Vec<String> {
    let mut out = vec![a.request.clone()];
    out.extend(a.fields.iter().map(|(n, v)| format!("{n}: {v}")));
    out.extend(a.units.iter().map(|u| format!("{u:?}")));
    out.extend(a.logs.iter().cloned());
    out
}

#[test]
fn the_door_states_nothing_of_the_decision_content() {
    let door = open(r#"{"models":{"jev":{"provider":"typesafe","upstream_model":"jev-2"}}}"#);
    let request = format!(r#"{{"model":"jev","state":{{"session":"{STATE_MARKER}"}}}}"#);
    let success = format!(
        r#"{{"request_id":"r1","usage":{{"units":3}},"answers":{{"decision":"{ANSWERS_MARKER}"}}}}"#
    );
    let echoed = format!(
        r#"{{"error":{{"code":"invalid_state"}},"state":{{"session":"{STATE_MARKER}"}},"answers":{{"x":"{ANSWERS_MARKER}"}}}}"#
    );
    // A success whose usage is unreadable: the door says so in its one log line.
    let unreadable = format!(
        r#"{{"usage":{{"units":"7"}},"answers":{{"decision":"{ANSWERS_MARKER}"}},"state":{{"session":"{STATE_MARKER}"}}}}"#
    );
    let mut said: Vec<String> = Vec::new();
    let mut logged = 0;
    let fixtures = [(200, &success), (422, &echoed), (200, &unreadable)];
    for (unit, (status, answer)) in fixtures.into_iter().enumerate() {
        let unit = unit as u64 + 1;
        let arrived = door.arrive_on(
            unit,
            "POST",
            "/v1/systemone",
            request.as_bytes(),
            &[("x-trace", "t-1")],
        );
        assert_eq!(arrived.outcome, Outcome::Ready, "{arrived:?}");
        said.push(arrived.entry.clone());
        said.push(arrived.text.clone());
        let mut bufs = Bufs::new(1 << 16);
        let attempt = door.piece(unit, Given::attempt(1), &mut bufs);
        said.extend(stated(&attempt));
        let caller = door.piece(unit, Given::caller(request.as_bytes()), &mut bufs);
        assert!(caller.far, "{caller:?}");
        // The relayed request is the caller's content: it carries the state, as it must.
        assert!(String::from_utf8_lossy(&caller.bytes).contains(STATE_MARKER));
        said.extend(stated(&caller));
        let answered = door.piece(unit, Given::whole(status, answer.as_bytes()), &mut bufs);
        assert_eq!(answered.outcome, Some(Outcome::Ready), "{answered:?}");
        // The relayed answer is the provider's content, unchanged.
        assert_eq!(answered.bytes, answer.as_bytes());
        logged += answered.logs.len();
        said.extend(stated(&answered));
    }
    assert!(logged > 0, "no fixture drove the door's log line");
    // A refusal the door renders states the kernel's words, never the unit's content: here of a
    // unit that arrived carrying the markers in its body and in a header the door keeps for its
    // far end, and was refused before any piece.
    let arrived = door.arrive_on(
        4,
        "POST",
        "/v1/systemone",
        request.as_bytes(),
        &[("x-trace", STATE_MARKER)],
    );
    assert_eq!(arrived.outcome, Outcome::Ready, "{arrived:?}");
    said.push(arrived.entry.clone());
    said.push(arrived.text.clone());
    let (outcome, rendered) = door.refusal(4, 403, "denied", true);
    assert_eq!(outcome, Outcome::Ready);
    said.push(String::from_utf8_lossy(&rendered).into_owned());

    // RED ARM: the witness sees a leak when one is there.
    assert!(leaks(&format!("x {STATE_MARKER} y")) && leaks(ANSWERS_MARKER));
    let leaked: Vec<&String> = said.iter().filter(|s| leaks(s)).collect();
    assert!(
        leaked.is_empty(),
        "the door stated the decision content: {leaked:?}"
    );
}
