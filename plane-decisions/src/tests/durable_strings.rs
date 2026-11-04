//! THE DURABLE-STRING PIN.

use crate::driven::tail;
use crate::meta::{CLASS_DECISION, PLANE_KEY};

#[test]
fn the_crate_is_the_renamed_one() {
    assert_eq!(env!("CARGO_PKG_NAME"), "busbar-plane-decisions");
}

#[test]
fn the_plane_key_stays_the_durable_string() {
    assert_eq!(PLANE_KEY, "decision");
}

#[test]
fn the_meter_class_stays_the_durable_string() {
    assert_eq!(CLASS_DECISION.as_str(), "decision");
    assert_eq!(
        tail::BILLABLE_CLASSES[tail::CLASS_DECISION_INDEX as usize]
            .0
            .as_str(),
        "decision"
    );
}
