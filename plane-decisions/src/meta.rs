//! What this plane is called, durably.
//!
//! Both strings are read once at registration and sealed into policy, and both are durable: a
//! stored meter row, a rate-card class and a registry key written under them must keep meaning this
//! plane, so the crate's rename to `decisions` touched neither (`src/tests/durable_strings.rs`).

use busbar_contract::ids::MeterClassId;

/// THE PLANE'S KEY: the durable string its Statement names it by.
pub const PLANE_KEY: &str = "decision";

/// The class a success's decision count is billed in: the hosted provider's `/usage/units`, the
/// upstream's own `/execution/states` for `nanojev`, or the plane's floor of one
/// ([`crate::driven::FarEndReading::counts`]).
pub const CLASS_DECISION: MeterClassId = MeterClassId::new("decision");
