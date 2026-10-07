//! This plane's answers to the kernel plane driver (`BUSBAR-1.6.0.md` Part 3, section 12,
//! "The plane driver"), in this plane's own vocabulary.
//!
//! The driver serves a unit in five crossings, and this module answers each one:
//!
//! | driver crossing | here |
//! |---|---|
//! | the Statement tail | [`tail`]: declaring and consumed sections, dialects, scope kinds, op classes, billable classes, fee units, needs, claims |
//! | `arrive` | [`arrive`]: the op class, the principal need and the dialect of a claimed request |
//! | `on_piece`, from the caller | [`caller_piece`]: the caller's whole body arrives as one piece; the kernel keeps it |
//! | `on_piece`, ATTEMPT (from the kernel) | [`attempt`]: the request bound for the far end, verb and target explicit |
//! | `on_piece`, from the far end | [`FarEndReading`]: the answer held to its last piece, then relayed unchanged with the units the far end reported |
//! | `refusal` | [`refusal_body`]: the kernel's status and text, in this dialect's error shape |
//!
//! Every answer is plain data. Nothing here holds a byte across calls: [`FarEndReading`] is a value
//! the caller owns for the length of one unit, bounded by [`ANSWER_BACKSTOP`], and it is the only
//! accumulating thing in the module.
//! Writing these answers into the host buffers of the plane ABI (`busbar_contract::abi::plane`) is
//! the plane door's job, one generic adapter for every plane; it is not this plane's.
//!
//! MODEL RESOLUTION (`BUSBAR-1.6.0.md` section 2, the decisions bullet; DECISIONS D8b): a request
//! routes by its top-level `model` ([`Models::resolve`]). One configured model and none named is the
//! default, and the body passes byte-identical; more than one and none named is `400`; a model this
//! generation does not configure is `404`. `upstream_model` rewrites ONLY the top-level `model`
//! value, by span splice ([`splice_model`]), when it is set and differs; otherwise the body passes
//! through. A generation with no model mounts no claim ([`served_claims`]), so its request falls
//! through to the kernel's unclaimed-route `404`, as before.
//!
//! WHAT THIS PLANE DOES NOT CLAIM: `GET /v1/models`. That path keeps its 1.5.5 bytes (the model
//! list busbar already serves there), so [`tail::CLAIMS`] and [`tail::OP_CLASSES`] carry `systemone` alone and
//! [`arrive`] answers the models operation as unclaimed.
//!
//! Money-blind, like the rest of the crate: the plane reports how many decision units the far end
//! said it used, in its one billable class, and never what they are worth.

use std::collections::BTreeMap;

use busbar_contract::ids::{MeterClassId, OpClassId};

use crate::codec::{
    self, error_body, CONTENT_TYPE_JSON, EGRESS_SCHEME, FIELD_CONTENT_TYPE, PTR_ERROR,
    PTR_USAGE_UNITS,
};
use crate::ops;

/// The plane's Statement tail, as data.
pub mod tail {
    use super::{MeterClassId, OpClassId};
    use crate::{claims, config, meta, ops};

    /// The top-level config section whose presence declares this plane.
    pub const SECTION_DECLARING: &str = config::SECTION;

    /// The sections this plane reads and does not own: every model names its provider there.
    pub const SECTIONS_CONSUMED: &[&str] = &["providers"];

    /// The one dialect this plane speaks. Dialect index 0 everywhere below.
    pub const DIALECTS: &[&str] = &[config::PROTOCOL];

    /// The outbound auth style a jev far end is reached under: the operator's credential as a
    /// bearer (the signed design: "auth = core gateway credential, plane returns a
    /// CredentialLocator, never sees the secret").
    pub const OUTBOUND_STYLE: &str = "bearer";

    /// Each dialect's default outbound auth style, `(dialect index, style)` (the design, "Outbound auth", step 2:
    /// a provider entry's `auth:`, else this; ARCHITECT Q-L1-AUTH (A), 2026-10-03).
    pub const DIALECT_AUTH: &[(u32, &str)] = &[(0, OUTBOUND_STYLE)];

    /// The resource granularity a grant names: a configured decision provider.
    pub const SCOPE_KINDS: &[&str] = &["decision_provider"];

    /// What one registration on this plane is called.
    pub const SUBJECT_NOUN: &str = "decision provider";

    /// The singular noun for one registration in admin responses.
    pub const ADMIN_NOUN: &str = "decision-provider";

    /// The record resource kind a registration is audited under: the scope kind.
    pub const AUDIT_KIND: &str = SCOPE_KINDS[0];

    /// The op classes this plane serves, in index order. `systemone` alone (see the module doc).
    pub const OP_CLASSES: &[OpClassId] = &[ops::OP_SYSTEMONE];

    /// The billable classes, in index order, each with its family: the far end's decision count,
    /// and the fee unit (also a class: the tail check holds `fee_units ⊆ billable_classes`).
    pub const BILLABLE_CLASSES: &[(MeterClassId, &str)] = &[
        (meta::CLASS_DECISION, "decision"),
        (MeterClassId::new(FEE_PER_REQUEST), FEE_FAMILY),
    ];

    /// The index of [`meta::CLASS_DECISION`] in [`BILLABLE_CLASSES`].
    pub const CLASS_DECISION_INDEX: u32 = 0;

    /// THE FEE UNIT: the request fee (`decisions.fees.per_request`), counted `1` on a unit whose
    /// far end answered a success, so a refused or failed unit owes none (the signed design C-3,
    /// "billable-success fee gated kernel-side", the tool plane's twin; ARCHITECT Q-L5-FEE (C)). A
    /// fee unit is no priced class: the rate card never names it.
    pub const FEE_PER_REQUEST: &str = busbar_contract::plane::PER_REQUEST;

    /// The family of the fee unit's class: a count of 0 or 1.
    pub const FEE_FAMILY: &str = "count";

    /// The index of the fee unit's class in [`BILLABLE_CLASSES`].
    pub const CLASS_FEE_INDEX: u32 = 1;

    /// The fee units this plane counts.
    pub const FEE_UNITS: &[&str] = &[FEE_PER_REQUEST];

    /// The connection needs, `(transport, auth)`, for the far-end direction: its dialect's style, the
    /// one a member resolves to by default (a member is bound on the need its resolved style names).
    /// The plane names the style and never holds what is behind it.
    pub const NEEDS: &[(&str, &str)] = &[(claims::TRANSPORT, OUTBOUND_STYLE)];

    /// The claims the generation snapshot publishes, `(verb, target)`.
    pub const CLAIMS: &[(&str, &str)] = &[("POST", ops::PATH_SYSTEMONE)];
}

/// The claims a generation's snapshot publishes, given how many `decisions.models` it configures:
/// [`tail::CLAIMS`] for one or more, none for none (see the module doc).
#[must_use]
pub fn served_claims(models: usize) -> &'static [(&'static str, &'static str)] {
    if models == 0 {
        &[]
    } else {
        tail::CLAIMS
    }
}

/// ONE GENERATION'S MODELS: each busbar-facing name, and the name its far end is sent instead where
/// the entry's `upstream_model` is set and differs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Models {
    by_name: BTreeMap<String, Option<String>>,
}

/// Where a request routes ([`Models::resolve`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Routed<'m> {
    /// The configured model it routes over.
    pub model: &'m str,
    /// The name the request's top-level `model` is rewritten to: set only when the request names a
    /// model and that model's `upstream_model` differs from it.
    pub upstream: Option<&'m str>,
}

/// Why a request routes nowhere ([`Models::resolve`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unrouted {
    /// It names no model, and more than one is configured: `400`.
    NoneNamed,
    /// Its top-level `model` is not a string: `400`.
    NotAName,
    /// It names a model this generation does not configure: `404`.
    Unknown,
}

impl Unrouted {
    /// The status the refusal wears.
    #[must_use]
    pub const fn status(self) -> u32 {
        match self {
            Self::NoneNamed | Self::NotAName => 400,
            Self::Unknown => 404,
        }
    }

    /// The words the refusal carries. They never echo the caller's model name.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::NoneNamed => {
                "the request names no model, and more than one decisions model is configured"
            }
            Self::NotAName => "the request's model is not a string",
            Self::Unknown => "the requested model is not configured",
        }
    }
}

impl Models {
    /// The models `section` configures.
    #[must_use]
    pub fn of(section: &crate::config::DecisionsSection) -> Self {
        Self {
            by_name: section
                .models
                .iter()
                .map(|(name, cfg)| {
                    let upstream = cfg.upstream_model.clone().filter(|u| u != name);
                    (name.clone(), upstream)
                })
                .collect(),
        }
    }

    /// How many models are configured.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// Whether none is.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Route a request by its top-level `model` ([`codec::PTR_MODEL`], read as metadata): the named
    /// model; with none named (absent or `null`), the one configured model, the default.
    ///
    /// # Errors
    ///
    /// [`Unrouted`]: none named with more than one configured, a `model` that is not a string, or a
    /// model this generation does not configure.
    pub fn resolve(&self, body: &[u8]) -> Result<Routed<'_>, Unrouted> {
        match codec::read_raw(body, codec::PTR_MODEL) {
            None | Some(b"null") => {
                let mut all = self.by_name.keys();
                match (all.next(), all.next()) {
                    (Some(one), None) => Ok(Routed {
                        model: one,
                        upstream: None,
                    }),
                    (Some(_), Some(_)) => Err(Unrouted::NoneNamed),
                    (None, _) => Err(Unrouted::Unknown),
                }
            }
            Some(_) => {
                let named = codec::read_str(body, codec::PTR_MODEL).ok_or(Unrouted::NotAName)?;
                let (model, upstream) =
                    self.by_name.get_key_value(named).ok_or(Unrouted::Unknown)?;
                Ok(Routed {
                    model,
                    upstream: upstream.as_deref(),
                })
            }
        }
    }
}

/// THE `upstream_model` SPLICE: `body` with its top-level `model` value replaced by `upstream`, as
/// a JSON string, and every other byte unchanged; `None` when the body has no top-level `model`
/// string (nothing to rewrite: the body passes through).
#[must_use]
pub fn splice_model(body: &[u8], upstream: &str) -> Option<Vec<u8>> {
    let span = codec::span_of(body, codec::PTR_MODEL)?;
    codec::read_str(body, codec::PTR_MODEL)?;
    let value = serde_json::to_string(upstream).ok()?;
    let mut out = Vec::with_capacity(body.len() - (span.end - span.start) + value.len());
    out.extend_from_slice(&body[..span.start]);
    out.extend_from_slice(value.as_bytes());
    out.extend_from_slice(&body[span.end..]);
    Some(out)
}

/// Whether the kernel must verify a principal before the first piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrincipalNeed {
    /// No principal.
    None,
    /// A principal is verified before the first piece.
    Required,
    /// A principal is used when one is presented.
    Optional,
}

/// A claimed request, classified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrived {
    /// The operation, by name.
    pub op: OpClassId,
    /// Its index in [`tail::OP_CLASSES`].
    pub op_class: u32,
    /// Whether a principal is verified first.
    pub principal: PrincipalNeed,
    /// Its index in [`tail::DIALECTS`].
    pub dialect: u32,
}

/// Classify an arriving request by its verb and target. `None` for anything this plane does not
/// claim, including `GET /v1/models`.
#[must_use]
pub fn arrive(verb: &str, target: &str) -> Option<Arrived> {
    let row = ops::row_for(verb, target)?;
    let op_class = tail::OP_CLASSES.iter().position(|op| *op == row.op)?;
    Some(Arrived {
        op: row.op,
        op_class: u32::try_from(op_class).ok()?,
        // A decision is billed to someone: the provider account is the deployment's, and the unit
        // is the caller's.
        principal: PrincipalNeed::Required,
        dialect: 0,
    })
}

/// What the plane says to a piece of the caller's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallerAnswer {
    /// Nothing to emit. The kernel keeps the body and re-pushes it on every attempt.
    Keep,
    /// The body ended empty: a `systemone` request carries the caller's state, so there is nothing
    /// to forward. The kernel refuses the unit.
    Empty,
}

/// Answer a caller piece. The kernel gathers the whole body and pushes it as one piece.
#[must_use]
pub fn caller_piece(bytes: &[u8], last: bool) -> CallerAnswer {
    if last && bytes.is_empty() {
        CallerAnswer::Empty
    } else {
        CallerAnswer::Keep
    }
}

/// The request bound for the far end, answered to an ATTEMPT piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FarEndRequest<'a> {
    /// The verb, explicit.
    pub verb: &'static str,
    /// The target, explicit.
    pub target: &'static str,
    /// The dialect fields. The kernel adds the auth fields for [`FarEndRequest::auth`].
    pub fields: [(&'static str, &'static [u8]); 1],
    /// The auth scheme the kernel decorates the request under.
    pub auth: &'static str,
    /// The caller's body, unchanged: byte identity is the dialect.
    pub body: &'a [u8],
}

/// Answer an ATTEMPT piece: the same request on every attempt, whichever member the kernel picked.
/// jev names no provider on the wire, so the member changes nothing in the request. Named
/// `attempt_request`, not `attempt`, so it is not a cross-plane re-spelling of llm/a2a's `attempt`
/// (structure-lint plane-dup; ARCHITECT ruling 2b 2026-10-04).
#[must_use]
pub fn attempt_request(caller_body: &[u8]) -> FarEndRequest<'_> {
    FarEndRequest {
        verb: "POST",
        target: ops::PATH_SYSTEMONE,
        fields: [(FIELD_CONTENT_TYPE, CONTENT_TYPE_JSON)],
        auth: EGRESS_SCHEME,
        body: caller_body,
    }
}

/// One unit count, as the plane reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Units {
    /// Index into [`tail::BILLABLE_CLASSES`].
    pub class: u32,
    /// Reported by the far end. The plane never estimates a decision count.
    pub reported: bool,
    /// The cumulative count.
    pub amount: u64,
}

/// THE ABUSE BACKSTOP on the one far-end answer a unit holds: past it the unit is refused, loudly,
/// and counted, and no other unit is touched. Decision answers are unbounded (#41, "jev ~5 KB
/// unbounded responses"), so this is no size policy: it is the backstop #41 allows, "a ceiling set
/// absurdly high that only a runaway/attack could hit; on trip it cleanly refuses THAT ONE request".
pub const ANSWER_BACKSTOP: usize = 256 * 1024 * 1024;

/// A far-end answer grew past [`ANSWER_BACKSTOP`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverBackstop;

/// The far end's answer, HELD until its last piece.
///
/// The count lives inside the body (`/usage/units`), so the answer is held whole and read once, and
/// only then relayed: an answer that cannot be billed is refused before a byte of it reaches the
/// caller. The held bytes are the one copy: the relay pays out of them, never out of a second
/// buffer (jev answers are not streamed, so holding them loses the caller nothing).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FarEndReading {
    body: Vec<u8>,
}

impl FarEndReading {
    /// A reading with nothing received.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Hold one far-end piece.
    ///
    /// # Errors
    ///
    /// [`OverBackstop`] when the answer would grow past [`ANSWER_BACKSTOP`]; nothing is held.
    pub fn piece(&mut self, bytes: &[u8]) -> Result<(), OverBackstop> {
        self.piece_within(bytes, ANSWER_BACKSTOP)
    }

    /// [`Self::piece`] under the backstop `cap`.
    pub(crate) fn piece_within(&mut self, bytes: &[u8], cap: usize) -> Result<(), OverBackstop> {
        if self.body.len().saturating_add(bytes.len()) > cap {
            return Err(OverBackstop);
        }
        self.body.extend_from_slice(bytes);
        Ok(())
    }

    /// The held answer, moved out whole for the relay (no copy).
    pub fn take_answer(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.body)
    }

    /// The units the far end reported, read at the last piece, by the rule the plane's
    /// `decode_response` reads them (`plane.rs`): a count is reported only for an answer with no
    /// `/error` member, whose `/usage/units` is a whole number a signed 64-bit count holds. An
    /// error answer, or a count that is missing, negative, fractional or past `i64::MAX`, reports
    /// nothing. The status plays no part: `decode_response` judges success by the `/error` member
    /// alone.
    #[must_use]
    pub fn units(&self) -> Option<Units> {
        if codec::has(&self.body, PTR_ERROR) {
            return None;
        }
        codec::read_u64(&self.body, PTR_USAGE_UNITS)
            .filter(|amount| i64::try_from(*amount).is_ok())
            .map(|amount| Units {
                class: tail::CLASS_DECISION_INDEX,
                reported: true,
                amount,
            })
    }

    /// THE REQUEST FEE, read at the last piece: `1` of [`tail::FEE_PER_REQUEST`] for an answer the
    /// far end gave as a success — a `2xx` status and no `/error` member, the success rule
    /// [`Self::units`] reads — and nothing otherwise, so a refused or failed unit owes no fee
    /// (billable success only).
    #[must_use]
    pub fn fee(&self, status: Option<u32>) -> Option<Units> {
        let success = status.is_some_and(|s| (200..300).contains(&s));
        (success && !codec::has(&self.body, PTR_ERROR)).then_some(Units {
            class: tail::CLASS_FEE_INDEX,
            reported: true,
            amount: 1,
        })
    }
}

/// THE CALLER FIELDS THE FAR END NEVER RECEIVES: the ones the plane writes itself (the document
/// type), the ones naming the hop (`host`), and the credential carriers (the far end is presented
/// the member's credential by the kernel, never the caller's). Compared without case. Hop-by-hop
/// fields and the body framing never reach the plane (the framer drops them).
pub const NOT_RELAYED: &[&str] = &[
    FIELD_CONTENT_TYPE,
    "host",
    "authorization",
    "proxy-authorization",
];

/// THE CALLER'S FIELDS THE FAR END RECEIVES, in the order the caller sent them, a repeated name
/// keeping every value: every field the caller sent but [`NOT_RELAYED`]'s (a same-dialect relay
/// passes every header but the governed set; ARCHITECT DEC-SERVE Q2, the OWNER's DIALECT-FIDELITY
/// F2). Names are lowercased, as the wire compares them.
#[must_use]
pub fn relayed_fields<'f>(
    caller: impl IntoIterator<Item = (&'f [u8], &'f [u8])>,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    caller
        .into_iter()
        .filter(|(name, _)| {
            !NOT_RELAYED
                .iter()
                .any(|n| name.eq_ignore_ascii_case(n.as_bytes()))
        })
        .map(|(name, value)| (name.to_ascii_lowercase(), value.to_vec()))
        .collect()
}

/// Render a refusal the kernel decided, in this dialect's error shape: `{"error": {"code", "message"}}`.
/// The kernel chose the status and wrote the text; the plane chooses only the code word for the
/// status.
#[must_use]
pub fn refusal_body(status: u16, text: &str) -> Vec<u8> {
    let code = match status {
        403 | 429 => "unsupported_operation",
        404 => "invalid_params",
        400..=499 => "invalid_request",
        500 => "internal",
        _ => "unsupported_operation",
    };
    error_body(code, text)
}

#[cfg(test)]
#[path = "tests/driven.rs"]
mod tests;
