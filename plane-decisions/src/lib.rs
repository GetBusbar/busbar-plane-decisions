//! The decisions plane: a request routed by its model to a decision provider, in either of the two
//! dialects it speaks, `jev` and `nanojev`.
//!
//! ## What this crate is
//!
//! ONE DOOR ([`door`], the crate's only public item): the plane kind's memory-ABI table, built on
//! the contract's SDK over this plane's answers to the plane driver (`driven.rs`). A busbar build
//! links it as a compiled-in row; the sibling `busbar-plane-decisions-plugin` cdylib exports the
//! same function as its one symbol, so the linked and the dropped-in door cannot answer
//! differently. The crate's one busbar dependency is `busbar-contract`; beside it, `serde` and
//! `serde_json` read the `decisions:` section and the handful of pointers the plane reads, and
//! `sonic_number` (ported from sonic-number 0.1.2) reads a number exactly as 1.5.5 did.
//!
//! Each dialect has one exact claim, its decision call (`jev`'s `POST /v1/systemone`, `nanojev`'s
//! `POST /api/evaluate`), published when a model of that dialect is configured. A request routes
//! over the model it names (or the only one of its dialect), its top-level `model` spliced to the
//! model's `upstream_model` where the two differ; the far end's answer is relayed unchanged.
//!
//! ## What this crate is not
//!
//! It holds no governance, no breaker, no signing key and no arithmetic over a metered quantity:
//! those are the kernel's. It states COUNTS, never a price: a success's decision count (the far
//! end's, or the plane's floor of one when the answer states none), the tokens the answer reports,
//! and its request fee. Nothing in this crate opens a socket, reads a file or reads a clock. The
//! deployment's hooks bind to its units as to every plane's: its `project` answers the hook view of
//! a request and applies a request-stage rewrite in the request's dialect.
//!
//! ## What it holds across calls
//!
//! The door's instance keeps, through the SDK's per-instance state: each live generation's snapshot
//! with the model routes it was built from (an arrival routes over the newest the host holds), each
//! provider's protocol as the host stated it at `open`, and each unit in flight from its arrival to
//! its end (its last far-end piece, its refusal or its cancel): the caller fields the far end
//! receives, the routed `upstream_model`, the far end's status and the fixed-space reading of its
//! answer. At most `MAX_UNITS` units live at once; past that an arrival is refused and no live unit
//! is dropped.
//!
//! ## The PII witness, stated once
//!
//! A decision request carries a caller-supplied `state` and the provider's answer carries its
//! `answers`: the decision content itself. The door relays both, and reads neither: the pointers it
//! resolves are a request's top-level `model` and an answer's usage counts (`codec.rs`). Nothing it
//! states of its own (an arrival's route and words, an answer's fields, counts and records) carries
//! those bytes. `tests/pii_witness.rs::the_door_states_nothing_of_the_decision_content` drives the
//! door over a request and an answer that carry both and fails on any of them in what it states.
//! The hook view is the one place the request reaches a hook, as every plane's does, and only a
//! hook the operator grants the prompt sees it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod claims;
mod codec;
mod config;
mod driven;
mod meta;
mod ops;
mod plane_door;
mod sonic_number;

/// THE DOOR: this plugin's one public item (BUSBAR-1.6.0.md l.3567, Crate shape).
pub use plane_door::door;

/// The crate's tests that reach past [`door`]: in-crate because the crate's only public item is the
/// door.
#[cfg(test)]
mod tests {
    /// The crate root's prose, held to the tree it describes.
    mod lib;

    mod durable_strings;
}
