//! The typed shape of the `decisions:` config section.
//!
//! The config-model ruling (owner, 2026-09-19, the jev v5 dictator sign-off) is explicit about
//! this section's shape: `DecisionsSection { models: Map<String, ModelCfg>, hooks,
//! upstream_credentials }`, reusing `ModelCfg` VERBATIM rather than inventing a second model type —
//! the uniform model-serving map applies to `pools` and `decisions` alike (per the ruling's
//! correction on the uniform `models` schema). This
//! is greenfield: `decisions` never shipped a `1.5.5` shape, so there is no migration to honor and
//! no byte-identity floor to hold — the section either parses or the whole document is refused, the
//! same fail-closed posture `deny_unknown_fields` gives every other section.
//!
//! `ModelCfg` is [`busbar_contract::config::ModelCfg`] (DECISIONS #40/#38 — moved out of
//! `busbar-kernel` so a plugin crate reusing it need not also depend on the kernel; see that
//! module's own doc). Same type, same field names, same wire bytes — only the module path moved.
//!
//! ## Canonical shape (from the ruling)
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
//! carries alongside its `models` map (the ruling's R4/R-C: the shared pair stays `{hooks,
//! upstream_credentials}`, with `models` itself reserved only inside model-serving planes). `hooks`
//! is a list of names into the top-level `hooks:` registry — this crate carries no `HookCfg` type of
//! its own (that lives in busbar-core, which this crate may not depend on), so the reserved member
//! is typed as the list of names a plane names its hooks by, exactly as `busbar-plane-a2a`'s own
//! `CONFIG_SCHEMA` declares its `hooks` member. `upstream_credentials` reuses
//! [`busbar_contract::config::UpstreamCreds`] verbatim — the same type `PoolCfg` already names for
//! its own per-section override — rather than a second `own`/`passthrough` enum.
//!
//! That type used to live in the legacy plugin-facing crate, and naming it was the ONLY reason this
//! plane depended on that crate — the only one of the five that did. The dependency was not free:
//! it reached banned source through `sha2 -> cpufeatures -> libc`, which the transitive source
//! denylist forbids any pure plugin kind, and which went unreported for as long as this crate was
//! in no kind list. The type moved to the contract beside `ModelCfg`, so both reserved shapes now
//! arrive from the one crate a plugin may name, and the edge is deleted rather than waived.

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

/// THE FIRST OF THE TWO WIRE DIALECTS THIS PLANE SPEAKS ([`PROTOCOLS`]): `jev`, the hosted
/// provider's.
///
/// Each dialect is a direct HTTP+JSON relay with no translating IR: there is no dialect adapter
/// behind it the way the LLM plane's `openai`/`anthropic`/… translators exist. BUSBAR-1.6.0.md #51
/// (OWNER-LOCKED 2026-09-20) rules the resolution/interpretation split explicitly: "the PLANE then
/// interprets the resolved dialect against what it supports: knows it ⇒ use it; doesn't ⇒ FAIL
/// (fail-closed)". Dialect validation is the plane's job, never the kernel's. These constants are
/// that knowledge, kept on the plane side of the seam so no kernel file ever spells the literal
/// `"jev"` (#49: the kernel names no plane or transport-protocol string).
pub const PROTOCOL: &str = "jev";

/// THE SECOND WIRE DIALECT (THE DESIGN section 2, the decisions bullet): "a second dialect exists
/// only where a provider's request spine really differs (NanoJev is the one, dialect `nanojev`:
/// `/api/evaluate` is its second exact claim; a model's dialect is its override or its provider's
/// protocol ...)".
pub const PROTOCOL_NANOJEV: &str = "nanojev";

/// Every dialect this plane speaks, in its dialect-index order.
pub const PROTOCOLS: &[&str] = &[PROTOCOL, PROTOCOL_NANOJEV];

/// THE DECLARING SECTION: the top-level config key whose presence declares this plane, beside
/// `pools:`, `tools:`, `agents:` and `streams:`. The plane owns it and says so here; the composition
/// root reads this constant for its registry declaration and spells no literal of its own.
///
/// Plural, and deliberately not the registry key: the key is the plane's identity (`decision`,
/// `PlaneMeta::KEY`) and the section is the operator's noun for the set of configured decision
/// models.
pub const SECTION: &str = "decisions";

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
