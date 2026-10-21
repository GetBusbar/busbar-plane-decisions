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
//! | `on_piece`, ATTEMPT (from the kernel) | [`attempt_request`]: the request bound for the far end, verb and target explicit |
//! | `on_piece`, from the far end | [`FarEndReading`]: the bytes relayed unchanged, and the units the answer bills |
//! | `refusal` | [`refusal_body`]: the kernel's status and text, in this dialect's error shape |
//! | `project` | [`project`]: the hook view of a request; [`rewritten`]: a hook's rewrite applied in the dialect |
//!
//! Every answer is plain data. Nothing here holds a byte across calls: [`FarEndReading`] is a value
//! the caller owns for the length of one unit, in fixed space, and it is the only accumulating
//! thing in the module.
//! Writing these answers into the host buffers of the plane ABI (`busbar_contract::abi::plane`) is
//! the plane door's job, one generic adapter for every plane; it is not this plane's.
//!
//! MODEL RESOLUTION (THE DESIGN section 2, the decisions bullet; DECISIONS D8b): "a request routes
//! by its model; one configured model and none named is the default (byte-identical); more than one
//! and none named is 400; an unknown model is 404. `upstream_model` rewrites only the top-level model
//! value, by span splice, when it is set and differs; otherwise the body passes through."
//!
//! | config | `POST /v1/systemone` |
//! |---|---|
//! | `decisions.models: {}` | `404`, the kernel's unclaimed-route body (nothing is claimed) |
//! | one model, none named | routed over that model, the body byte for byte |
//! | several models, none named | `400` ([`Unrouted::Unnamed`]) |
//! | a model named | routed over it; a name no model has is `404` ([`Unrouted::Unknown`]) |
//!
//! With no model configured the snapshot publishes no claim ([`served_claims`]), so the request
//! falls through to the kernel's unclaimed-route body,
//! `{"error":{"code":null,"message":"the requested resource was not found","param":null,"type":"not_found_error"}}`,
//! byte for byte as before. The top-level `model` ([`codec::PTR_MODEL`]) is the one request member
//! this plane reads, and only to route ([`route_model`]) and splice ([`spliced`]).
//!
//! THE SECOND DIALECT (THE DESIGN section 2): "NanoJev is the one, dialect `nanojev`: `/api/evaluate`
//! is its second exact claim; a model's dialect is its override or its provider's protocol; a model
//! of the wrong dialect gets that dialect's existing unknown-model refusal; it meters the upstream's
//! own billing unit, counted from the answer already read (`execution.states`; an answer with no
//! count refuses loudly ...), with no extra egress". Each dialect's claim is published when the
//! generation configures a model of that dialect ([`served_claims`]); a request routes among the
//! models of the dialect its claim is ([`route_model`]), so a model of the other dialect is that
//! dialect's unknown model (`404`). Its answer is read by its dialect's count
//! ([`FarEndReading::for_dialect`]): `/execution/states`, required.
//!
//! WHAT THIS PLANE DOES NOT CLAIM: `GET /v1/models`. That path keeps its 1.5.5 bytes (the model
//! list busbar already serves there, which appends this plane's listed models), so
//! [`tail::CLAIMS`] and [`tail::OP_CLASSES`] carry each dialect's decision call (`systemone`,
//! `evaluate`) and no models operation, and [`arrive`] answers `GET /v1/models` as unclaimed.
//!
//! Money-blind, like the rest of the crate: the plane reports what a successful answer bills, in its
//! billable classes, and never what it is worth (THE DESIGN section 7, the owner money rulings
//! 2026-09-28: "bills one `decision` per successful, settled answer from every provider; its card
//! prices `decision`, `input_tokens` and `output_tokens` (family `decision`, all three required, 0
//! = free); the hosted provider keeps 1.5.5's `/usage/units` as the decision count").

use busbar_contract::ids::{MeterClassId, OpClassId};

use crate::codec::{
    self, error_body, CONTENT_TYPE_JSON, EGRESS_SCHEME, FIELD_CONTENT_TYPE, PTR_EXECUTION_STATES,
    PTR_USAGE_INPUT_TOKENS, PTR_USAGE_OUTPUT_TOKENS, PTR_USAGE_UNITS,
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

    /// The dialects this plane speaks, in index order: `jev` ([`DIALECT_JEV`]) and `nanojev`
    /// ([`DIALECT_NANOJEV`]).
    pub const DIALECTS: &[&str] = config::PROTOCOLS;

    /// The index of `jev` in [`DIALECTS`].
    pub const DIALECT_JEV: u32 = 0;

    /// The index of `nanojev` in [`DIALECTS`].
    pub const DIALECT_NANOJEV: u32 = 1;

    /// The outbound auth style a jev far end is reached under: the operator's credential as a
    /// bearer (the signed design: "auth = core gateway credential, plane returns a
    /// CredentialLocator, never sees the secret").
    pub const OUTBOUND_STYLE: &str = "bearer";

    /// Each dialect's default outbound auth style, `(dialect index, style)` (the design, "Outbound auth", step 2:
    /// a provider entry's `auth:`, else this; ARCHITECT Q-L1-AUTH (A), 2026-10-03).
    pub const DIALECT_AUTH: &[(u32, &str)] = &[
        (DIALECT_JEV, OUTBOUND_STYLE),
        (DIALECT_NANOJEV, OUTBOUND_STYLE),
    ];

    /// The resource granularity a grant names: a configured decision provider.
    pub const SCOPE_KINDS: &[&str] = &["decision_provider"];

    /// What one registration on this plane is called.
    pub const SUBJECT_NOUN: &str = "decision provider";

    /// The singular noun for one registration in admin responses.
    pub const ADMIN_NOUN: &str = "decision-provider";

    /// The record resource kind a registration is audited under: the scope kind.
    pub const AUDIT_KIND: &str = SCOPE_KINDS[0];

    /// The op classes this plane serves, in index order: each dialect's decision call (see the
    /// module doc).
    pub const OP_CLASSES: &[OpClassId] = &[ops::OP_SYSTEMONE, ops::OP_EVALUATE];

    /// The billable classes, in index order, each with its family: the decision count, the fee
    /// unit (also a class: the tail check holds `fee_units ⊆ billable_classes`), and the far end's
    /// input and output tokens. THE DESIGN section 7 (owner money rulings 2026-09-28): the card
    /// "prices `decision`, `input_tokens` and `output_tokens` (family `decision`, all three
    /// required, 0 = free)".
    pub const BILLABLE_CLASSES: &[(MeterClassId, &str)] = &[
        (meta::CLASS_DECISION, DECISION_FAMILY),
        (MeterClassId::new(FEE_PER_REQUEST), FEE_FAMILY),
        (CLASS_INPUT_TOKENS, DECISION_FAMILY),
        (CLASS_OUTPUT_TOKENS, DECISION_FAMILY),
    ];

    /// The family every priced class of this plane is in.
    pub const DECISION_FAMILY: &str = "decision";

    /// The far end's input tokens, where its answer reports them (`/usage/input_tokens`).
    pub const CLASS_INPUT_TOKENS: MeterClassId = MeterClassId::new("input_tokens");

    /// The far end's output tokens, where its answer reports them (`/usage/output_tokens`).
    pub const CLASS_OUTPUT_TOKENS: MeterClassId = MeterClassId::new("output_tokens");

    /// The index of [`meta::CLASS_DECISION`] in [`BILLABLE_CLASSES`].
    pub const CLASS_DECISION_INDEX: u32 = 0;

    /// The index of [`CLASS_INPUT_TOKENS`] in [`BILLABLE_CLASSES`].
    pub const CLASS_INPUT_TOKENS_INDEX: u32 = 2;

    /// The index of [`CLASS_OUTPUT_TOKENS`] in [`BILLABLE_CLASSES`].
    pub const CLASS_OUTPUT_TOKENS_INDEX: u32 = 3;

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

    /// The claims a generation snapshot may publish, `(verb, target, dialect)`: each dialect's one
    /// exact claim.
    pub const CLAIMS: &[(&str, &str, u32)] = &[
        ("POST", ops::PATH_SYSTEMONE, DIALECT_JEV),
        ("POST", ops::PATH_EVALUATE, DIALECT_NANOJEV),
    ];
}

/// The claims a generation's snapshot publishes, given the dialect of each model it configures:
/// each dialect's claim when at least one model is of it, none for none (see the module doc).
#[must_use]
pub fn served_claims(dialects: &[u32]) -> Vec<(&'static str, &'static str, u32)> {
    tail::CLAIMS
        .iter()
        .filter(|(_, _, dialect)| dialects.contains(dialect))
        .copied()
        .collect()
}

/// A protocol's index in [`tail::DIALECTS`], where this plane speaks it.
#[must_use]
pub fn dialect_of(protocol: &str) -> Option<u32> {
    tail::DIALECTS
        .iter()
        .position(|d| *d == protocol)
        .and_then(|i| u32::try_from(i).ok())
}

/// Why a claimed request routes over no configured model (THE DESIGN section 2: "more than one and
/// none named is 400; an unknown model is 404").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unrouted {
    /// The request names no model and the generation configures several.
    Unnamed,
    /// The request's `model` is not a string.
    NotAName,
    /// The request names a model the generation does not configure.
    Unknown(String),
    /// The request's `model` is the empty string: 1.5.5's missing-model refusal (v1.5.5
    /// `crates/busbar/src/ingress/dispatch.rs:138-152`, a model that is empty is none).
    EmptyModel,
    /// The body is no JSON document 1.5.5 would read: malformed, or nested past 1.5.5's parse depth
    /// ([`MAX_PARSE_DEPTH`]).
    Unparsable,
}

impl Unrouted {
    /// The status the refusal wears.
    #[must_use]
    pub const fn status(&self) -> u16 {
        match self {
            Self::Unnamed | Self::NotAName | Self::EmptyModel | Self::Unparsable => 400,
            Self::Unknown(_) => 404,
        }
    }

    /// The words the caller reads, in the dialect's error shape ([`refusal_body`]).
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Unnamed => "the request names no model, and several decisions models are \
                              configured: name one in `model`"
                .to_string(),
            // 1.5.5 read a `model` that is not a string as none (v1.5.5 ingress/dispatch.rs:130-152).
            Self::NotAName => MISSING_MODEL.to_string(),
            Self::Unknown(model) => format!("{UNKNOWN_HEAD}{model}{UNKNOWN_TAIL}"),
            Self::Unparsable => UNPARSABLE.to_string(),
            Self::EmptyModel => MISSING_MODEL.to_string(),
        }
    }
}

/// The words 1.5.5 refused a body it could not parse in, at 400 (v1.5.5
/// `crates/busbar/src/ingress/dispatch.rs:116` and `ingress/mod.rs:650`, kind `invalid_request`).
pub const UNPARSABLE: &str = "We could not parse the JSON body of your request.";

/// The words 1.5.5 refused a request with an empty `model` in, at 400 (v1.5.5
/// `crates/busbar/src/ingress/dispatch.rs:152`).
pub const MISSING_MODEL: &str = "Missing required parameter: 'model'.";

/// THE PARSE DEPTH 1.5.5 READ A BODY TO (v1.5.5 `crates/busbar/src/json.rs`, `MAX_JSON_DEPTH`):
/// a body nested deeper is refused before any document is built, as one that does not parse.
pub const MAX_PARSE_DEPTH: usize = 128;

/// 1.5.5's depth scan (v1.5.5 `json.rs`, `exceeds_max_depth`): whether `bytes` nests `{`/`[` past
/// `max`, brackets inside strings (and escaped quotes) not counted.
fn exceeds_depth(bytes: &[u8], max: usize) -> bool {
    let (mut depth, mut in_string, mut escaped) = (0_usize, false, false);
    for &b in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > max {
                    return true;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// The body as 1.5.5 read it, for a body the span scanner cannot walk: the whole document, or
/// [`Unrouted::Unparsable`] for one 1.5.5 refused (malformed, or past [`MAX_PARSE_DEPTH`]).
///
/// # Errors
///
/// [`Unrouted::Unparsable`], as above.
pub fn parsed(body: &[u8]) -> Result<serde_json::Value, Unrouted> {
    if exceeds_depth(body, MAX_PARSE_DEPTH) {
        return Err(Unrouted::Unparsable);
    }
    // 1.5.5 validated the whole input as UTF-8 before it parsed it (sonic-rs `from_slice`).
    let text = core::str::from_utf8(body).map_err(|_| Unrouted::Unparsable)?;
    // The depth is bounded by the scan above, at 1.5.5's own; serde's limit stops one level short.
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let value = serde::Deserialize::deserialize(&mut de).map_err(|_| Unrouted::Unparsable)?;
    de.end().map_err(|_| Unrouted::Unparsable)?;
    Ok(value)
}

/// The unknown-model refusal's words before the echoed name: 1.5.5's (v1.5.5 ingress/mod.rs
/// `not_found_message`), and predev's decisions refusal, byte for byte.
const UNKNOWN_HEAD: &str = "The model '";

/// The unknown-model refusal's words after the echoed name.
const UNKNOWN_TAIL: &str = "' does not exist or you do not have access to it.";

/// The most bytes of a requested model's name an unknown-model refusal echoes: every byte the
/// refusal's words may run (`busbar_contract::abi::plane::MAX_REFUSAL_TEXT`, past which the `arrive`
/// is a FAULT) but the sentence's own. Every name up to it is echoed whole, as 1.5.5 and predev
/// echoed it; only a name the refusal could not carry is cut, on a character boundary, and marked
/// cut.
pub const MAX_ECHOED_MODEL: usize = busbar_contract::abi::plane::MAX_REFUSAL_TEXT as usize
    - UNKNOWN_HEAD.len()
    - UNKNOWN_TAIL.len();

/// The mark a cut name ends in.
const CUT_MARK: &str = "...";

/// What an unknown-model refusal echoes of `name`: the whole name up to [`MAX_ECHOED_MODEL`] bytes,
/// else its first bytes to a character boundary, followed by `...`, within the same bound.
#[must_use]
pub fn echoed(name: &str) -> String {
    if name.len() <= MAX_ECHOED_MODEL {
        return name.to_string();
    }
    let mut cut = MAX_ECHOED_MODEL - CUT_MARK.len();
    while !name.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{CUT_MARK}", &name[..cut])
}

/// THE TOP-LEVEL KEYS 1.5.5 DECODED while it validated a body (v1.5.5
/// `crates/busbar/src/proxy/lazy_body.rs:41-52`, `captured_head_keys`): `model`, `stream`,
/// `stream_options`, `system` and the array-stream shim key. Their values were decoded whole (so a
/// lone surrogate escape in one refused the body); every other value was only scanned.
const HEAD_KEYS: &[&str] = &[
    "__busbar_gemini_json_array",
    "model",
    "stream",
    "stream_options",
    "system",
];

/// THE BODY AS 1.5.5 READ IT AT INGRESS (v1.5.5 `crates/busbar/src/ingress/dispatch.rs:101-116`,
/// `LazyBody::parse`): at most [`MAX_PARSE_DEPTH`] deep, UTF-8 throughout, then one validating
/// parse that builds no document (trailing bytes, a bad number, a bad escape or a truncated document
/// refuse), decoding the top-level keys and the values of [`HEAD_KEYS`] (`lazy_body.rs:122-127`,
/// `Head`). Answers the top-level `model`'s value, the last one where it repeats (1.5.5's map
/// insert); `None` when the body names none or is no object.
///
/// # Errors
///
/// [`Unrouted::Unparsable`]: 1.5.5's 400.
pub fn readable(body: &[u8]) -> Result<Option<serde_json::Value>, Unrouted> {
    if exceeds_depth(body, MAX_PARSE_DEPTH) {
        return Err(Unrouted::Unparsable);
    }
    let text = core::str::from_utf8(body).map_err(|_| Unrouted::Unparsable)?;
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let model = serde::Deserializer::deserialize_any(&mut de, TopLevel)
        .map_err(|_| Unrouted::Unparsable)?;
    de.end().map_err(|_| Unrouted::Unparsable)?;
    Ok(model)
}

/// [`readable`]'s walk of the top-level value: an object's keys decoded, the values of
/// [`HEAD_KEYS`] decoded, every other value scanned. Answers the `model` value.
struct TopLevel;

impl<'de> serde::de::Visitor<'de> for TopLevel {
    type Value = Option<serde_json::Value>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON document")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut model = None;
        while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
            if key == "model" {
                model = Some(map.next_value::<serde_json::Value>()?);
            } else if HEAD_KEYS.contains(&&*key) {
                map.next_value::<serde_json::Value>()?;
            } else {
                map.next_value::<serde::de::IgnoredAny>()?;
            }
        }
        Ok(model)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
        Ok(None)
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
}

/// The top-level `model` a request body names, read as 1.5.5 read it ([`readable`]): `Ok(None)`
/// when it names none.
///
/// # Errors
///
/// [`Unrouted::Unparsable`] for a body 1.5.5 could not parse, [`Unrouted::NotAName`] for a
/// `model` that is not a string, and [`Unrouted::EmptyModel`] for an empty one (both 1.5.5's
/// missing-model 400).
pub fn requested_model(body: &[u8]) -> Result<Option<std::borrow::Cow<'_, str>>, Unrouted> {
    match readable(body)? {
        None => Ok(None),
        Some(serde_json::Value::String(name)) if name.is_empty() => Err(Unrouted::EmptyModel),
        Some(serde_json::Value::String(name)) => Ok(Some(std::borrow::Cow::Owned(name))),
        Some(_) => Err(Unrouted::NotAName),
    }
}

/// THE MODEL A CLAIMED REQUEST ROUTES OVER among the `models` its claim's dialect `serves` (the
/// generation's configured names, each with what this plane keeps of it): the one it names, else
/// the only one of that dialect configured. Answers the configured name and its entry. A model of
/// another dialect is this dialect's unknown model (THE DESIGN section 2: "a model of the wrong
/// dialect gets that dialect's existing unknown-model refusal").
///
/// # Errors
///
/// [`Unrouted`]: several configured and none named, a `model` that is not a string, or a name none
/// of the dialect's models has.
pub fn route_model<'m, V>(
    body: &[u8],
    models: &'m std::collections::BTreeMap<String, V>,
    serves: impl Fn(&V) -> bool,
) -> Result<(&'m str, &'m V), Unrouted> {
    match requested_model(body)? {
        Some(name) => models
            .get_key_value(name.as_ref())
            .filter(|(_, v)| serves(v))
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| Unrouted::Unknown(echoed(&name))),
        None => {
            let mut all = models.iter().filter(|(_, v)| serves(v));
            match (all.next(), all.next()) {
                (Some((k, v)), None) => Ok((k.as_str(), v)),
                _ => Err(Unrouted::Unnamed),
            }
        }
    }
}

/// THE `upstream_model` SPLICE: `body` with its top-level `model` value replaced by `upstream`, when
/// the body names a model and it differs from `upstream`; `None` when the body passes through
/// unchanged. Every byte outside the value's span is kept: member order, whitespace and the rest of
/// the document are the caller's.
#[must_use]
pub fn spliced(body: &[u8], upstream: &str) -> Option<Vec<u8>> {
    // The body was held to the document 1.5.5 read at `arrive`: it is not judged again here.
    let span = match busbar_contract::spans::resolve_pointer(body, codec::PTR_MODEL) {
        busbar_contract::spans::Resolved::Found(span) => span,
        busbar_contract::spans::Resolved::Missing => return None,
        busbar_contract::spans::Resolved::Malformed
        | busbar_contract::spans::Resolved::NeedMore => {
            // The scanner cannot walk the body: the model is set in the document as 1.5.5 read
            // it, every other member keeping its value.
            let mut document = parsed(body).ok()?;
            let named = document.get("model")?.as_str()?;
            if named == upstream {
                return None;
            }
            document.as_object_mut()?.insert(
                "model".to_string(),
                serde_json::Value::String(upstream.to_string()),
            );
            return Some(document.to_string().into_bytes());
        }
    };
    let named: String = serde_json::from_slice(body.get(span.start..span.end)?).ok()?;
    if named == upstream {
        return None;
    }
    let value = serde_json::to_string(upstream).ok()?;
    let mut out = Vec::with_capacity(body.len() - (span.end - span.start) + value.len());
    out.extend_from_slice(body.get(..span.start)?);
    out.extend_from_slice(value.as_bytes());
    out.extend_from_slice(body.get(span.end..)?);
    Some(out)
}

/// WHETHER [`spliced`] CAN SET `upstream` IN `body` WHERE IT MUST, as 1.5.5 could: a body naming a
/// model other than `upstream` is one 1.5.5 rewrote on its hop, which materialized the whole
/// document; one whose document would not parse (a string its head never decoded but the document
/// parse refuses) was answered 500 "We received an unexpected internal error. Please try again."
/// (v1.5.5 `crates/busbar/src/proxy/engine/mod.rs:1499-1520`, `DETAIL_INTERNAL_ERROR` at
/// `proxy/wire.rs:726`). `arrive` answers it so, never relaying the caller's model unspliced. A body
/// naming no model, or `upstream` itself, needs no rewrite.
#[must_use]
pub fn spliceable(body: &[u8], upstream: &str) -> bool {
    match readable(body) {
        Ok(Some(serde_json::Value::String(named))) if named != upstream => parsed(body).is_ok(),
        _ => true,
    }
}

/// 1.5.5's words for a request whose document would not parse where it had to be rewritten for its
/// hop (v1.5.5 `proxy/wire.rs:726`), at 500.
pub const INTERNAL_ERROR: &str = "We received an unexpected internal error. Please try again.";

/// The dialect whose claim `target` names (its path; a query is not the path), where one does.
#[must_use]
pub fn dialect_at(target: &str) -> Option<u32> {
    let path = target.split_once('?').map_or(target, |(path, _)| path);
    tail::CLAIMS
        .iter()
        .find(|(_, t, _)| *t == path)
        .map(|(_, _, dialect)| *dialect)
}

/// The role the one turn of a projected request is shown under: the caller's.
pub const PROJECTED_ROLE: &str = "user";

/// What a hook is shown of one request ([`project`]): the entry it routes over, its dialect's name
/// and the characters of the request document, which is the view's one turn and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projected<'r> {
    /// The configured model the request routes over; empty when it routes over none.
    pub entry: &'r str,
    /// The dialect's name ([`tail::DIALECTS`]).
    pub dialect: &'static str,
    /// The request document's characters.
    pub chars: usize,
}

/// PROJECT one request of `dialect` for the hooks: the model it routes over among the `routes` of
/// that dialect ([`route_model`]; `dialect_of` reads a route's dialect), the dialect's name and the
/// size of the document. The document itself is the view's one turn and its body, as the caller
/// sent it (or as a rewrite left it); the plane reads nothing of it for itself beyond its top-level
/// `model`, and mints no fact from it.
#[must_use]
pub fn project<'r, V>(
    request: &[u8],
    dialect: u32,
    routes: &'r std::collections::BTreeMap<String, V>,
    dialect_of: impl Fn(&V) -> u32,
) -> Projected<'r> {
    let entry =
        route_model(request, routes, |r| dialect_of(r) == dialect).map_or("", |(name, _)| name);
    let chars = chars_of(request);
    Projected {
        entry,
        dialect: tail::DIALECTS
            .get(dialect as usize)
            .copied()
            .unwrap_or_default(),
        chars,
    }
}

/// The characters of a request document, as the hook view counts them.
#[must_use]
pub fn chars_of(request: &[u8]) -> usize {
    match core::str::from_utf8(request) {
        Ok(text) => text.chars().count(),
        Err(_) => String::from_utf8_lossy(request).chars().count(),
    }
}

/// The document a request-stage hook's rewrite names (1.5.5's rewrite JSON, `{"messages": [...],
/// "tools": [...]}`, as the kernel hands it): the last message whose `content` is a JSON object,
/// or a string holding one, or a message with no `role` that is itself an object. `None` when no
/// message names one.
fn rewrite_document(rewrite: &[u8]) -> Option<serde_json::Map<String, serde_json::Value>> {
    let reply: serde_json::Value = serde_json::from_slice(rewrite).ok()?;
    let mut document = None;
    for message in reply.get("messages")?.as_array()? {
        let candidate = match message.get("content") {
            Some(content) => content,
            None if message.get("role").is_none() => message,
            None => continue,
        };
        let resolved = match candidate {
            serde_json::Value::Object(map) => Some(map.clone()),
            serde_json::Value::String(text) => serde_json::from_str(text).ok(),
            _ => None,
        };
        if resolved.is_some() {
            document = resolved;
        }
    }
    document
}

/// A rewrite the plane will not apply: its document names a top-level `model` other than the model
/// the request is routed over. The model is no rewritable member (a rewrite never switches the
/// provider a request goes to); the rewrite is that hook's FAILURE, which the kernel answers by the
/// hook's own `on_error`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRewritten {
    /// The model the document names.
    pub named: String,
}

/// THE REWRITE, APPLIED IN THE DIALECT, as 1.5.5 applied one: the rewrite's document
/// ([`rewrite_document`]) REPLACES the request wholesale (v1.5.5 `RewriteReply`: "the replacement
/// message body"). A member the document omits is dropped, so a hook redacts by omission; a
/// member it states is set to its value, the caller's own bytes kept for a value it leaves as it
/// was; a member the caller did not send is added after the caller's. The top-level `model` alone
/// is not the document's: it stays the caller's whether or not the document repeats it, and a
/// document naming another model is refused ([`ModelRewritten`]), never a silent switch of
/// provider. `routed` is the model entry the request is routed over.
///
/// `Ok(None)` when the rewrite names no document, the body is not one JSON object, or the document
/// is the request as it stands: the request goes on byte for byte as it was (1.5.5: a rewrite that
/// cannot be applied leaves the request unmodified).
///
/// # Errors
///
/// [`ModelRewritten`]: the document names a model other than `routed`.
pub fn rewritten(
    body: &[u8],
    rewrite: &[u8],
    routed: &str,
) -> Result<Option<Vec<u8>>, ModelRewritten> {
    let Some(mut document) = rewrite_document(rewrite) else {
        return Ok(None);
    };
    const MODEL: &str = "model";
    // The caller's `model`, as `arrive` read it (1.5.5's head).
    let Ok(caller_model) = readable(body) else {
        return Ok(None);
    };
    if let Some(named) = document.remove(MODEL) {
        let same = named.as_str() == Some(routed) || caller_model.as_ref() == Some(&named);
        if !same {
            return Err(ModelRewritten {
                named: named
                    .as_str()
                    .map_or_else(|| named.to_string(), str::to_string),
            });
        }
    }
    // Read to 1.5.5's depth (the request was held to it at `arrive`), not serde's one short.
    let current = match parsed(body) {
        Ok(serde_json::Value::Object(map)) => map,
        Ok(_) => return Ok(None),
        // The request's document does not parse (a string 1.5.5's head never decoded, which the
        // document parse refuses): the rewrite still applies, and the request is never forwarded
        // un-rewritten. The hook's document replaces it whole, the caller's `model` kept.
        Err(_) => {
            let mut whole = document;
            if let Some(model) = caller_model {
                whole.insert(MODEL.to_string(), model);
            }
            return Ok(Some(
                serde_json::Value::Object(whole).to_string().into_bytes(),
            ));
        }
    };
    // The document as it replaces the request: the caller's `model`, then the document's members.
    let unchanged = current.len() == document.len() + usize::from(current.contains_key(MODEL))
        && document.iter().all(|(k, v)| current.get(k) == Some(v));
    if unchanged {
        return Ok(None);
    }
    Ok(Some(replaced(body, &current, &document)))
}

/// [`rewritten`]'s request: the caller's members `document` keeps (the `model` always), in the
/// caller's order, each the caller's own bytes where its value is unchanged, then the members the
/// caller did not send. A rewrite the hook answered ALWAYS applies: when a member's bytes cannot be
/// read back (the span scanner refuses a body nested past its depth, which serde reads), the
/// replacement is written from the document itself, as 1.5.5 wrote the rewritten value, and only
/// the caller's exact bytes of the unchanged members are lost.
fn replaced(
    body: &[u8],
    current: &serde_json::Map<String, serde_json::Value>,
    document: &serde_json::Map<String, serde_json::Value>,
) -> Vec<u8> {
    spliced_members(body, current, document).unwrap_or_else(|| {
        let mut whole = document.clone();
        if let Some(model) = current.get("model") {
            whole.insert("model".to_string(), model.clone());
        }
        serde_json::Value::Object(whole).to_string().into_bytes()
    })
}

/// [`replaced`] in the caller's bytes; `None` when a member's span cannot be read.
fn spliced_members(
    body: &[u8],
    current: &serde_json::Map<String, serde_json::Value>,
    document: &serde_json::Map<String, serde_json::Value>,
) -> Option<Vec<u8>> {
    let span_of = |key: &str| {
        let pointer = format!("/{}", key.replace('~', "~0").replace('/', "~1"));
        match busbar_contract::spans::resolve_pointer(body, &pointer) {
            busbar_contract::spans::Resolved::Found(span) => Some(span),
            _ => None,
        }
    };
    let mut kept: Vec<(usize, Vec<u8>)> = Vec::with_capacity(current.len());
    for (key, value) in current {
        let span = span_of(key)?;
        let bytes = if key == "model" || document.get(key) == Some(value) {
            body.get(span.start..span.end)?.to_vec()
        } else if let Some(wanted) = document.get(key) {
            wanted.to_string().into_bytes()
        } else {
            continue;
        };
        kept.push((span.start, member(key, &bytes)));
    }
    kept.sort_by_key(|(at, _)| *at);
    let mut members: Vec<Vec<u8>> = kept.into_iter().map(|(_, m)| m).collect();
    for (key, value) in document {
        if !current.contains_key(key) {
            members.push(member(key, value.to_string().as_bytes()));
        }
    }
    let mut out = Vec::with_capacity(body.len());
    out.push(b'{');
    out.extend_from_slice(&members.join(&b','));
    out.push(b'}');
    Some(out)
}

/// One member, `"key":value`, `value` as written.
fn member(key: &str, value: &[u8]) -> Vec<u8> {
    let mut m = serde_json::Value::String(key.to_string())
        .to_string()
        .into_bytes();
    m.push(b':');
    m.extend_from_slice(value);
    m
}

/// Whether the kernel must verify a principal before the first piece. Every claimed request of this
/// plane needs one: a decision is billed to someone (see [`arrive`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrincipalNeed {
    /// A principal is verified before the first piece.
    Required,
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

/// Classify an arriving request by its verb and target: its operation, and the dialect its claim
/// is. `None` for anything this plane does not claim, including `GET /v1/models`. The kernel
/// matches a claim on the request path and hands the whole target, so a query is not the path.
#[must_use]
pub fn arrive(verb: &str, target: &str) -> Option<Arrived> {
    let target = target.split_once('?').map_or(target, |(path, _)| path);
    let row = ops::row_for(verb, target)?;
    let op_class = tail::OP_CLASSES.iter().position(|op| *op == row.op)?;
    let dialect = tail::CLAIMS
        .iter()
        .find(|(v, t, _)| *t == target && v.eq_ignore_ascii_case(verb))?
        .2;
    Some(Arrived {
        op: row.op,
        op_class: u32::try_from(op_class).ok()?,
        // A decision is billed to someone: the provider account is the deployment's, and the unit
        // is the caller's.
        principal: PrincipalNeed::Required,
        dialect,
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

/// Answer an ATTEMPT piece of a unit of `dialect`: the same request on every attempt, whichever
/// member the kernel picked, at the dialect's own path. Neither dialect names a provider on the
/// wire, so the member changes nothing in the request. Named `attempt_request`, not `attempt`, so
/// it is not a cross-plane re-spelling of llm/a2a's `attempt` (structure-lint plane-dup; ARCHITECT
/// ruling 2b 2026-10-04).
#[must_use]
pub fn attempt_request(dialect: u32, caller_body: &[u8]) -> FarEndRequest<'_> {
    let target = tail::CLAIMS
        .iter()
        .find(|(_, _, d)| *d == dialect)
        .map_or(ops::PATH_SYSTEMONE, |(_, t, _)| t);
    FarEndRequest {
        verb: "POST",
        target,
        fields: [(FIELD_CONTENT_TYPE, CONTENT_TYPE_JSON)],
        auth: EGRESS_SCHEME,
        body: caller_body,
    }
}

/// Where a count the plane reports comes from (THE DESIGN section 7 keeps the far end's own counts
/// apart from the plane's floor; both bill).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The far end's answer states it (`UNITS_REPORTED`); the request fee is reported so too, as
    /// the contract reports a fee unit incurred (0 or 1 on its class).
    Reported,
    /// The plane's own floor for a success whose answer states no count (`UNITS_FLOOR`): one
    /// decision.
    Floor,
}

/// One unit count, as the plane reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Units {
    /// Index into [`tail::BILLABLE_CLASSES`].
    pub class: u32,
    /// Where the count comes from.
    pub source: Source,
    /// The cumulative count.
    pub amount: u64,
}

/// A usage member the far end's success answer carries but that is not a whole count: the money
/// rule's loud failure (finding 395, kept: "a usage member present but not a whole count is
/// LOCATED, never dropped"), never a silent 0. Names the member's pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnreadableUsage(pub &'static str);

/// What one answer bills, in [`tail::BILLABLE_CLASSES`]' order of reading: the decision count,
/// the input and output tokens, and the request fee. `None` = nothing in that class.
pub type Counts = [Option<Units>; 4];

/// A usage member as the answer states it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Member {
    /// The answer has no such member.
    #[default]
    Absent,
    /// A whole count.
    Whole(u64),
    /// Present, and not a whole count.
    Unreadable,
}

/// What a member of the answer is to the reading: `/error`, `/usage`, one of the three usage
/// counts (by its index in its dialect's [`Usage::counted`]), or none of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Slot {
    #[default]
    Other,
    Error,
    Usage,
    Count(usize),
}

/// Where a dialect's answer states its counts: the top-level member holding them, and the members
/// under it, `(name, pointer)`, in [`Counts`]' order (decision, input tokens, output tokens).
/// Usage pointers are dialect data (THE DESIGN section 2).
struct Usage {
    parent: &'static [u8],
    counted: &'static [(&'static [u8], &'static str)],
    /// The decision count is required: an answer with none refuses loudly.
    required: bool,
}

/// Each dialect's [`Usage`], in [`tail::DIALECTS`]' order: `jev` reads `/usage/units` (one decision
/// when absent) and its tokens; `nanojev` reads the upstream's own billing unit,
/// `/execution/states`, and an answer with no count refuses loudly.
const USAGE: [Usage; 2] = [
    Usage {
        parent: b"usage",
        counted: &[
            (b"units", PTR_USAGE_UNITS),
            (b"input_tokens", PTR_USAGE_INPUT_TOKENS),
            (b"output_tokens", PTR_USAGE_OUTPUT_TOKENS),
        ],
        required: false,
    },
    Usage {
        parent: b"execution",
        counted: &[(b"states", PTR_EXECUTION_STATES)],
        required: true,
    },
];

/// The longest member name compared ("output_tokens"), with room.
const KEY_CAP: usize = 16;
/// The longest scalar a whole count is spelled in (`u64::MAX` is 20 digits), with room.
const SCALAR_CAP: usize = 24;

/// The far end's answer, read as it is relayed.
///
/// Every piece is relayed to the caller unchanged, as it arrives, and read as it passes: a scan of
/// the JSON structure that keeps only what the money rule reads (whether `/error` is there and not
/// `null`, and the dialect's counts: `/usage/*`, or `/execution/states`), in fixed space. No byte of the answer is kept, so an
/// answer of any size is read in the same few bytes (THE DESIGN section 11.2: "no allocation and no
/// blocking on the hot path"), and the pieces may split the answer anywhere.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FarEndReading {
    /// The nesting depth: `1` inside the top-level value.
    depth: u32,
    /// Whether the container at depth 1 and at depth 2 is an object.
    object: [bool; 3],
    /// In an object at depth 1 or 2: the next string is a member name.
    want_key: bool,
    /// Inside a string; the string is a member name; the last byte was its escape.
    in_str: bool,
    str_key: bool,
    escaped: bool,
    /// The member name being read (cut at [`KEY_CAP`]: a longer one is none compared).
    key: [u8; KEY_CAP],
    key_len: usize,
    /// The member a value at depth 1, and at depth 2, belongs to.
    at: [Slot; 3],
    /// The scalar being read for `scalar_of` (cut at [`SCALAR_CAP`]: a longer one is no count).
    scalar: [u8; SCALAR_CAP],
    scalar_len: usize,
    scalar_of: Option<Slot>,
    /// `/error`: absent (`None`), `null` (`Some(false)`), or there (`Some(true)`).
    error: Option<bool>,
    /// The counts, in the dialect's [`Usage::counted`] order.
    members: [Member; 3],
    /// The dialect the answer is in ([`USAGE`]'s index).
    dialect: u32,
}

impl FarEndReading {
    /// A `jev` reading with nothing received.
    #[cfg(test)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A reading of an answer in `dialect` ([`tail::DIALECTS`]' index), nothing received.
    #[must_use]
    pub fn for_dialect(dialect: u32) -> Self {
        Self {
            dialect,
            ..Self::default()
        }
    }

    /// The dialect's [`Usage`].
    fn usage(&self) -> &'static Usage {
        USAGE.get(self.dialect as usize).unwrap_or(&USAGE[0])
    }

    /// Take one far-end piece and return the bytes to relay to the caller: the piece itself.
    pub fn piece<'p>(&mut self, bytes: &'p [u8]) -> &'p [u8] {
        for &b in bytes {
            self.byte(b);
        }
        bytes
    }

    /// The member at the current depth a value starting now belongs to.
    fn slot(&self) -> Slot {
        match self.depth {
            1 if self.object[1] => self.at[1],
            2 if self.object[2] && self.at[1] == Slot::Usage => self.at[2],
            _ => Slot::Other,
        }
    }

    /// Whether the container the reading is in is an object it reads member names of.
    fn in_object(&self) -> bool {
        matches!(self.depth, 1 | 2) && self.object[self.depth as usize]
    }

    /// A value starts: `scalar` = it is a bare scalar (a number, `true`, `false`, `null`).
    fn value(&mut self, scalar: Option<u8>) {
        let slot = self.slot();
        match (slot, scalar) {
            (Slot::Error, None) => self.error = Some(true),
            (Slot::Count(i), None) => self.members[i] = Member::Unreadable,
            (Slot::Error | Slot::Count(_), Some(b)) => {
                self.scalar[0] = b;
                self.scalar_len = 1;
                self.scalar_of = Some(slot);
            }
            _ => {}
        }
    }

    /// The scalar being read, ended.
    fn end_scalar(&mut self) {
        let Some(slot) = self.scalar_of.take() else {
            return;
        };
        let raw = &self.scalar[..self.scalar_len];
        match slot {
            Slot::Error => self.error = Some(raw != b"null"),
            Slot::Count(i) => {
                self.members[i] = if self.scalar_len > SCALAR_CAP - 1 {
                    Member::Unreadable
                } else {
                    whole(raw).map_or(Member::Unreadable, Member::Whole)
                };
            }
            _ => {}
        }
    }

    /// A member name, read whole.
    fn end_key(&mut self) {
        let key = &self.key[..self.key_len.min(KEY_CAP)];
        let named = if self.key_len > KEY_CAP {
            Slot::Other
        } else if self.depth == 1 {
            match key {
                b"error" => Slot::Error,
                k if k == self.usage().parent => Slot::Usage,
                _ => Slot::Other,
            }
        } else {
            self.usage()
                .counted
                .iter()
                .position(|(name, _)| *name == key)
                .map_or(Slot::Other, Slot::Count)
        };
        if let 1 | 2 = self.depth {
            self.at[self.depth as usize] = named;
        }
    }

    /// One byte of the answer.
    fn byte(&mut self, b: u8) {
        if self.in_str {
            if self.escaped {
                self.escaped = false;
            } else if b == b'"' {
                self.in_str = false;
                if self.str_key {
                    self.end_key();
                }
                return;
            } else if b == b'\\' {
                self.escaped = true;
            }
            if self.str_key {
                if self.key_len < KEY_CAP {
                    self.key[self.key_len] = b;
                }
                self.key_len = self.key_len.saturating_add(1);
            }
            return;
        }
        if self.scalar_of.is_some() {
            if !matches!(
                b,
                b' ' | b'\t' | b'\n' | b'\r' | b',' | b'}' | b']' | b':' | b'"' | b'{' | b'['
            ) {
                if self.scalar_len < SCALAR_CAP {
                    self.scalar[self.scalar_len] = b;
                    self.scalar_len += 1;
                }
                return;
            }
            self.end_scalar();
        }
        match b {
            b' ' | b'\t' | b'\n' | b'\r' | b':' => {}
            b'"' => {
                self.in_str = true;
                self.escaped = false;
                self.str_key = self.in_object() && self.want_key;
                if self.str_key {
                    self.key_len = 0;
                    self.want_key = false;
                } else {
                    self.value(None);
                }
            }
            b'{' | b'[' => {
                self.value(None);
                self.depth = self.depth.saturating_add(1);
                if let 1 | 2 = self.depth {
                    self.object[self.depth as usize] = b == b'{';
                    self.at[self.depth as usize] = Slot::Other;
                }
                self.want_key = b == b'{';
            }
            b'}' | b']' => {
                self.depth = self.depth.saturating_sub(1);
                self.want_key = false;
            }
            b',' => self.want_key = self.in_object(),
            _ => self.value(Some(b)),
        }
    }

    /// The reading as the answer stands: a scalar the answer ended in is read as it is.
    fn settled(&self) -> (Option<bool>, [Member; 3]) {
        let mut done = self.clone();
        done.end_scalar();
        (done.error, done.members)
    }

    /// Whether the answer, with the far end's `status`, is a SUCCESS: a `2xx` status and no
    /// `/error` member (a `null` one is none). The one success rule every count below reads.
    #[must_use]
    pub fn success(&self, status: Option<u32>) -> bool {
        status.is_some_and(|s| (200..300).contains(&s)) && self.settled().0 != Some(true)
    }

    /// WHAT THE ANSWER BILLS, read at the last piece (THE DESIGN section 7, the owner money rulings):
    /// a non-success bills nothing; a success bills one `decision` (the plane's floor,
    /// [`Source::Floor`]), or the whole count its
    /// `/usage/units` reports (the hosted provider's count, DECISIONS D9), its `/usage/input_tokens`
    /// and `/usage/output_tokens` where it reports them, and the request fee (`1` of
    /// [`tail::FEE_PER_REQUEST`]: a refused or failed unit owes none).
    ///
    /// # Errors
    ///
    /// [`UnreadableUsage`]: a success whose usage member is present but is not a whole count a
    /// signed 64-bit count holds (negative, fractional, a string, `null`, past `i64::MAX`).
    pub fn counts(&self, status: Option<u32>) -> Result<Counts, UnreadableUsage> {
        if !self.success(status) {
            return Ok([None; 4]);
        }
        let (_, members) = self.settled();
        let usage = self.usage();
        let mut whole = [None; 3];
        for (i, (_, pointer)) in usage.counted.iter().enumerate() {
            whole[i] = match members[i] {
                Member::Absent if i == 0 && usage.required => return Err(UnreadableUsage(pointer)),
                Member::Absent => None,
                Member::Whole(n) => Some(n),
                Member::Unreadable => return Err(UnreadableUsage(pointer)),
            };
        }
        let reported = |class: u32, amount: u64| Units {
            class,
            source: Source::Reported,
            amount,
        };
        // A success whose answer states no decision count bills the plane's floor, one decision,
        // stated as the plane's, never as the far end's.
        let decisions = whole[0].map_or(
            Units {
                class: tail::CLASS_DECISION_INDEX,
                source: Source::Floor,
                amount: 1,
            },
            |n| reported(tail::CLASS_DECISION_INDEX, n),
        );
        Ok([
            Some(decisions),
            whole[1].map(|n| reported(tail::CLASS_INPUT_TOKENS_INDEX, n)),
            whole[2].map(|n| reported(tail::CLASS_OUTPUT_TOKENS_INDEX, n)),
            Some(reported(tail::CLASS_FEE_INDEX, 1)),
        ])
    }
}

/// A usage member's raw bytes read as a whole count a signed 64-bit count holds: ASCII digits
/// alone, at most `i64::MAX`.
#[must_use]
pub fn whole(raw: &[u8]) -> Option<u64> {
    if raw.is_empty() || !raw.iter().all(u8::is_ascii_digit) {
        return None;
    }
    core::str::from_utf8(raw)
        .ok()?
        .parse::<u64>()
        .ok()
        .filter(|n| i64::try_from(*n).is_ok())
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

/// The caller's fields the far end receives, held in one buffer per unit: each field's name
/// (lowercased) and value, back to back, and where each ends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelayedFields {
    bytes: Vec<u8>,
    /// Per field, `(end of its name, end of its value)` in `bytes`.
    ends: Vec<(usize, usize)>,
}

impl RelayedFields {
    /// Every field, `(name, value)`, in the caller's order.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> + '_ {
        let mut from = 0;
        self.ends.iter().map(move |&(name, value)| {
            let field = (&self.bytes[from..name], &self.bytes[name..value]);
            from = value;
            field
        })
    }

    /// Whether no field is relayed.
    #[cfg(test)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
}

/// Whether the caller's field `name` reaches the far end.
fn relayed(name: &[u8]) -> bool {
    !NOT_RELAYED
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n.as_bytes()))
}

/// THE CALLER'S FIELDS THE FAR END RECEIVES, in the order the caller sent them, a repeated name
/// keeping every value: every field the caller sent but [`NOT_RELAYED`]'s (a same-dialect relay
/// passes every header but the governed set; ARCHITECT DEC-SERVE Q2, the OWNER's DIALECT-FIDELITY
/// F2). Names are lowercased, as the wire compares them. Kept in one buffer sized once.
#[must_use]
pub fn relayed_fields<'f, I>(caller: I) -> RelayedFields
where
    I: IntoIterator<Item = (&'f [u8], &'f [u8])>,
    I::IntoIter: Clone,
{
    let fields = caller.into_iter();
    let (mut size, mut count) = (0, 0);
    for (name, value) in fields.clone().filter(|(name, _)| relayed(name)) {
        size += name.len() + value.len();
        count += 1;
    }
    let mut kept = RelayedFields {
        bytes: Vec::with_capacity(size),
        ends: Vec::with_capacity(count),
    };
    for (name, value) in fields.filter(|(name, _)| relayed(name)) {
        kept.bytes.extend(name.iter().map(u8::to_ascii_lowercase));
        let name_end = kept.bytes.len();
        kept.bytes.extend_from_slice(value);
        kept.ends.push((name_end, kept.bytes.len()));
    }
    kept
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
