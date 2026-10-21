// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! The plane is pure, at the type level: the same question gets the same answer, the clock moves
//! nothing, and the key is the durable one. Once `tests/purity.rs`'s; in-crate since the crate's
//! only public item is its door (the source scans stay in `tests/purity.rs`).

use busbar_contract::plane::{Ingress, Plane, PlaneMeta};
use busbar_contract::wire::FrameCursor;

use super::common::{frame, Scaffold};
use crate::DecisionPlane;

/// The decode step gives the same answer every time it is asked the same question.
#[test]
fn the_decode_step_is_deterministic() {
    let plane = DecisionPlane::EMPTY;
    let body = br#"{"state":{"a":1},"context":{}}"#;
    let mut answers = Vec::new();
    for _ in 0..8 {
        let scaffold = Scaffold::new("http")
            .with_method("POST")
            .on_path("/v1/systemone");
        let ctx = scaffold.ctx();
        let frames = vec![frame(body)];
        let mut cursor = FrameCursor::new(&frames);
        let ingress = plane
            .decode_ingress(&mut cursor, None, &ctx)
            .expect("a known surface decodes");
        let summary = match ingress {
            Ingress::OneShot(d) => format!("{:?}/{}", d.op, d.facts.len()),
            other => format!("{other:?}"),
        };
        answers.push(summary);
    }
    assert!(
        answers.windows(2).all(|w| w[0] == w[1]),
        "the decode step varied over one body: {answers:?}"
    );
}

/// The encode step writes the same bytes every time it is asked the same question.
#[test]
fn the_encode_step_is_deterministic() {
    let plane = DecisionPlane::EMPTY;
    let answer = br#"{"request_id":"r1","usage":{"units":1}}"#;
    let mut written = Vec::new();
    for _ in 0..8 {
        let scaffold = Scaffold::new("http");
        let ctx = scaffold.ctx();
        let r = busbar_contract::plane::Response {
            ir: busbar_contract::bounded::Ir::new(answer, &[]),
            finish: busbar_contract::unit::FinishClass::Complete,
            facts: busbar_contract::bounded::Facts::new(),
        };
        let out = plane
            .encode_response(&r, None, &ctx)
            .expect("it re-encodes");
        written.push(out.as_slice().to_vec());
    }
    assert!(
        written.windows(2).all(|w| w[0] == w[1]),
        "the encode step varied over one answer"
    );
}

/// The plane does not read the clock, so a call at a different time gives the same answer.
#[test]
fn the_answer_does_not_move_with_the_clock() {
    let plane = DecisionPlane::EMPTY;
    let body = br#"{"state":{"a":1}}"#;
    let mut answers = Vec::new();
    for unix_secs in [2_000_000_000_u64, 1_000_000_000] {
        let scaffold = Scaffold::new("http")
            .with_method("POST")
            .on_path("/v1/systemone");
        let ctx = busbar_contract::unit::Ctx::new(
            busbar_contract::unit::Clock {
                unix_secs,
                monotonic_nanos: 0,
            },
            &scaffold.config,
            Some(&scaffold.session),
            &scaffold.transport,
            &scaffold.labels,
            &scaffold.arena,
            &scaffold.host,
        );
        assert_eq!(ctx.clock().unix_secs, unix_secs);
        let frames = vec![frame(body)];
        let mut cursor = FrameCursor::new(&frames);
        let Ok(Ingress::OneShot(draft)) = plane.decode_ingress(&mut cursor, None, &ctx) else {
            panic!("a systemone POST with a body decodes as one shot");
        };
        answers.push(format!("{:?}", draft.op));
    }
    assert_eq!(
        answers[0], answers[1],
        "the decoded answer moved with the clock: {answers:?}"
    );
}

/// The PlaneMeta constants are actually reachable on the plane value — a smoke check that the
/// crate wires its own trait impl where the other tests assume it does.
#[test]
fn plane_meta_key_is_decision() {
    assert_eq!(<DecisionPlane as PlaneMeta>::KEY, "decision");
}
