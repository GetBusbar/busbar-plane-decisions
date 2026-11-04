//! What this plane is called, durably.

use busbar_contract::ids::MeterClassId;

/// THE PLANE'S KEY: the durable string its Statement names it by.
pub const PLANE_KEY: &str = "decision";

/// The class a success's decision count is billed in: the hosted provider's `/usage/units`, the
/// upstream's own `/execution/states` for `nanojev`, or the plane's floor of one
/// ([`crate::driven::FarEndReading::counts`]).
pub const CLASS_DECISION: MeterClassId = MeterClassId::new("decision");
