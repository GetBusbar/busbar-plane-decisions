//! The typed shape of the `decisions:` config section.
//!
//! `DecisionsSection { models: Map<String, ModelCfg>, hooks, upstream_credentials }` reuses
//! [`busbar_contract::config::ModelCfg`] rather than a second model type, as `pools` does. The
//! section either parses or the whole document is refused, the same fail-closed posture
//! `deny_unknown_fields` gives every other section.
//!
//! ## Canonical shape
//!
//! ```yaml
//! decisions:
//!   models:
//!     jev: { provider: typesafe, upstream_model: jev-1.13.0 }
//! ```
//!
//! `jev` here is the busbar-facing model name a caller targets; `provider` names a `providers:`
//! entry (the transport-agnostic connection — base_url, error_map, credential ref); `upstream_model`
//! is the real string sent to the provider, omitted meaning "send the key verbatim".
//!
//! ## Reserved sibling keys
//!
//! `hooks` and `upstream_credentials` are the two reserved members every model-serving section
//! carries alongside its `models` map. `hooks` is a list of names into the top-level `hooks:`
//! registry (this crate carries no `HookCfg` type of its own). `upstream_credentials` reuses
//! [`busbar_contract::config::UpstreamCreds`].

use std::collections::HashMap;

use serde::Deserialize;

use busbar_contract::config::{ModelCfg, UpstreamCreds};

/// The `decisions:` section, typed.
///
/// `deny_unknown_fields`: a typo'd top-level member of `decisions:` must fail boot, not be silently
/// ignored — the same posture every other section in this workspace takes.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DecisionsSection {
    /// The busbar-facing model names this deployment exposes on the decisions plane, each bound to a
    /// provider connection. REQUIRED-shaped in practice (an empty map means no `jev:` target is
    /// reachable, which is a valid but inert configuration rather than a refusal).
    #[serde(default)]
    pub models: HashMap<String, ModelCfg>,
    /// The reserved hook-name list every section carries: the section-level attach. Each entry
    /// names a top-level `hooks:` registration; the kernel judges that the name resolves (boot and
    /// `--validate` refuse one that does not), resolves the hooks it names, and binds them to every
    /// decisions unit beside the deployment's global hooks, over the view this plane's `project`
    /// answers. The door never reads it itself.
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    pub hooks: Vec<String>,
    /// The reserved upstream-credential override: `own` (this deployment's own credential, the
    /// default) or `passthrough` (relay the caller's). `None` means inherit whatever the
    /// deployment-wide default is.
    ///
    /// The kernel reads it off the folded section; the door never does.
    #[serde(default)]
    #[cfg_attr(not(test), allow(dead_code))]
    pub upstream_credentials: Option<UpstreamCreds>,
}

/// The first of the two wire dialects this plane speaks ([`PROTOCOLS`]): `jev`, the hosted
/// provider's.
///
/// Each dialect is a direct HTTP+JSON relay with no translating IR. The plane interprets the
/// resolved dialect against what it supports and fails closed on one it does not know; the kernel
/// never spells the literal.
pub const PROTOCOL: &str = "jev";

/// The second wire dialect, `nanojev`, whose request spine differs from `jev`'s; `/api/evaluate`
/// is its exact claim.
pub const PROTOCOL_NANOJEV: &str = "nanojev";

/// Every dialect this plane speaks, in its dialect-index order.
pub const PROTOCOLS: &[&str] = &[PROTOCOL, PROTOCOL_NANOJEV];

/// The top-level config key whose presence declares this plane.
///
/// Plural, and deliberately not the registry key: the key is the plane's identity (`decision`,
/// `PlaneMeta::KEY`) and the section is the operator's noun for the set of configured decision
/// models.
pub const SECTION: &str = "decisions";

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
