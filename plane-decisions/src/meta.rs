//! What this plane is called, durably.

use busbar_contract::ids::MeterClassId;

/// THE PLANE'S KEY: the durable string its Statement names it by.
pub const PLANE_KEY: &str = "decision";

/// The class a success's decision count is billed in: the hosted provider's `/usage/units`, the
pub const CLASS_DECISION: MeterClassId = MeterClassId::new("decision");
