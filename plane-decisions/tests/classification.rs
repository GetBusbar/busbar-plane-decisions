// SPDX-License-Identifier: Apache-2.0
//! The classification operation through the ordinary plane door, including incurred usage.
#![allow(unsafe_code)]
mod door_harness;
use busbar_contract::abi::mechanism::call::Outcome;
use busbar_contract::abi::plane::UNITS_REPORTED;
use door_harness::{open, Bufs, Given};
use serde_json::{json, Value};
const SETTINGS: &str = r#"{"models":{"j":{"provider":"typesafe","upstream_model":"pinned-jev"},"n":{"provider":"np","protocol":"nanojev"}}}"#;
fn request(model: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"version":1,"model":model,"messages":[{"role":"user","text":"hello"}],"tiers":[{"label":"easy","description":"simple"},{"label":"hard","description":"complex"}]})).unwrap()
}
#[test]
fn classification_uses_selected_dialect_and_normalizes_without_raw_provider_data() {
    for (model, dialect, target) in [
        ("j", 0, "POST /v1/systemone"),
        ("n", 1, "POST /api/evaluate"),
    ] {
        let door = open(SETTINGS);
        let body = request(model);
        let arrived = door.arrive_on(1, "POST", "/busbar/decisions/v1/classify", &body, &[]);
        assert_eq!(arrived.outcome, Outcome::Ready);
        assert_eq!(arrived.dialect, dialect);
        let mut bufs = Bufs::new(65_536);
        let mut attempt = Given::attempt(1);
        attempt.dialect = dialect;
        assert_eq!(door.piece(1, attempt, &mut bufs).request, target);
        let mut caller = Given::caller(&body);
        caller.dialect = dialect;
        let sent = door.piece(1, caller, &mut bufs);
        assert!(sent.far);
        let sent: Value = serde_json::from_slice(&sent.bytes).unwrap();
        if dialect == 0 {
            assert_eq!(sent["model"], "pinned-jev");
            assert_eq!(sent["questions"]["complexity"]["type"], "choice");
        } else {
            assert_eq!(sent["states"][0]["id"], "complexity");
        }
        let answer = json!({"type":"choice","choice":"easy","confidence":0.8,"probabilities":{"easy":0.8,"hard":0.2},"private_provider_field":"never-relayed"});
        let raw = if dialect == 0 {
            json!({"answers":{"complexity":answer},"usage":{"units":3}})
        } else {
            json!({"states":[{"id":"complexity","answers":{"complexity":answer}}],"execution":{"states":3}})
        };
        let raw = serde_json::to_vec(&raw).unwrap();
        let split = raw.len() / 2;
        let mut first = Given::far(Some(200), false, &raw[..split]);
        first.dialect = dialect;
        assert!(door.piece(1, first, &mut bufs).bytes.is_empty());
        let mut last = Given::far(None, true, &raw[split..]);
        last.dialect = dialect;
        let out = door.piece(1, last, &mut bufs);
        assert!(out.done);
        assert_eq!(out.status, 200);
        assert!(out.units.contains(&(0, UNITS_REPORTED, 3)));
        let result: Value = serde_json::from_slice(&out.bytes).unwrap();
        assert_eq!(result["label"], "easy");
        assert_eq!(result["version"], 1);
        assert!(!String::from_utf8(out.bytes)
            .unwrap()
            .contains("never-relayed"));
    }
}
#[test]
fn classification_malformed_answer_still_bills_reported_work() {
    let door = open(SETTINGS);
    let body = request("j");
    assert_eq!(
        door.arrive_on(1, "POST", "/busbar/decisions/v1/classify", &body, &[])
            .outcome,
        Outcome::Ready
    );
    let mut bufs = Bufs::new(65_536);
    door.piece(1, Given::attempt(1), &mut bufs);
    door.piece(1, Given::caller(&body), &mut bufs);
    let out=door.piece(1,Given::whole(200,br#"{"answers":{"complexity":{"type":"choice","choice":"injected-model","confidence":1}},"usage":{"units":7}}"#),&mut bufs);
    assert_eq!(out.status, 502);
    assert!(out.units.contains(&(0, UNITS_REPORTED, 7)));
    assert!(!String::from_utf8(out.bytes)
        .unwrap()
        .contains("injected-model"));
}
