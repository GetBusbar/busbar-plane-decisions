//! This plane's answers to the kernel plane driver, in this plane's own vocabulary.
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
//! Model resolution: a request routes by its model; one configured model and none named is the
//! default (byte-identical); more than one and none named is 400; an unknown model is 404.
//! `upstream_model` rewrites only the top-level model value, by span splice, when it is set and
//! differs; otherwise the body passes through.
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
//! `{"error":{"code":null,"message":"the requested resource was not found","param":null,"type":"not_found_error"}}`.
//! The top-level `model` ([`codec::PTR_MODEL`]) is the one request member
//! this plane reads, and only to route ([`route_model`]) and splice ([`spliced`]).
//!
//! The second dialect, `nanojev`: `/api/evaluate` is its exact claim; a model's dialect is its
//! override or its provider's protocol; a model of the wrong dialect gets that dialect's existing
//! unknown-model refusal; it meters the upstream's own billing unit, counted from the answer
//! already read (`execution.states`; an answer with no count refuses loudly), with no extra
//! egress. Each dialect's claim is published when the
//! generation configures a model of that dialect ([`served_claims`]); a request routes among the
//! models of the dialect its claim is ([`route_model`]), so a model of the other dialect is that
//! dialect's unknown model (`404`). Its answer is read by its dialect's count
//! ([`FarEndReading::for_dialect`]): `/execution/states`, required.
//!
//! Not claimed: `GET /v1/models`. That path keeps the model list busbar serves there (which
//! appends this plane's listed models), so
//! [`tail::CLAIMS`] and [`tail::OP_CLASSES`] carry each dialect's decision call (`systemone`,
//! `evaluate`) and no models operation, and [`arrive`] answers `GET /v1/models` as unclaimed.
//!
//! Money-blind, like the rest of the crate: the plane reports what a successful answer bills, in its
//! billable classes, and never what it is worth: it bills one `decision` per successful, settled
//! answer from every provider, and the rate card prices `decision`, `input_tokens` and
//! `output_tokens` (family `decision`, all three required, 0 = free); the hosted provider's
//! `/usage/units` is the decision count.

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
    /// bearer, returned as a locator the plane never resolves.
    pub const OUTBOUND_STYLE: &str = "bearer";

    /// Each dialect's default outbound auth style, `(dialect index, style)`: a provider entry's
    /// `auth:` wins, else this.
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
    /// input and output tokens. The card prices `decision`, `input_tokens` and `output_tokens`
    /// (family `decision`, all three required, 0 = free).
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

    /// The fee unit: the request fee (`decisions.fees.per_request`), counted `1` on a unit whose
    /// far end answered a success, so a refused or failed unit owes none. A fee unit is no priced
    /// class: the rate card never names it.
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

/// Why a claimed request routes over no configured model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unrouted {
    /// The request names no model and the generation configures several.
    Unnamed,
    /// The request's `model` is not a string.
    NotAName,
    /// The request names a model the generation does not configure.
    Unknown(String),
    /// The request's `model` is the empty string: a model that is empty is none.
    EmptyModel,
    /// The body is no JSON document busbar reads: malformed, or nested past the parse depth
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
            // A `model` that is not a string reads as none.
            Self::NotAName => MISSING_MODEL.to_string(),
            Self::Unknown(model) => format!("{UNKNOWN_HEAD}{model}{UNKNOWN_TAIL}"),
            Self::Unparsable => UNPARSABLE.to_string(),
            Self::EmptyModel => MISSING_MODEL.to_string(),
        }
    }
}

/// The words a body that cannot be parsed is refused in, at 400.
pub const UNPARSABLE: &str = "We could not parse the JSON body of your request.";

/// The words a request with an empty `model` is refused in, at 400.
pub const MISSING_MODEL: &str = "Missing required parameter: 'model'.";

/// The parse depth a body is read to: a body nested deeper is refused before any document is
/// built, as one that does not parse.
pub const MAX_PARSE_DEPTH: usize = 128;

/// The body as 1.5.5 read it, for a body the span scanner cannot walk: the whole document, or
/// [`Unrouted::Unparsable`] for one 1.5.5 refused (malformed, or past [`MAX_PARSE_DEPTH`]).
///
/// # Errors
///
/// [`Unrouted::Unparsable`], as above.
pub fn parsed(body: &[u8]) -> Result<serde_json::Value, Unrouted> {
    // The document 1.5.5 would build ([`valid`]), and only then built: by serde, or where serde
    // refuses a number 1.5.5's reader took as finite through its exponent cap ([`built`]), from
    // that reader's own values.
    if !valid(body) {
        return Err(Unrouted::Unparsable);
    }
    let text = core::str::from_utf8(body).map_err(|_| Unrouted::Unparsable)?;
    // The depth is bounded by the scan above, at 1.5.5's own; serde's limit stops one level short.
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    let read: Result<serde_json::Value, _> = serde::Deserialize::deserialize(&mut de);
    match read {
        Ok(value) if de.end().is_ok() => Ok(value),
        // A document 1.5.5 built whose number serde refuses (one 1.5.5's reader took as finite
        // through its exponent cap, its true value overflowing): built as 1.5.5 built it.
        _ => built(body).ok_or(Unrouted::Unparsable),
    }
}

/// An open container of [`built`]: an array's items so far, or an object's members so far and
/// the key of the member being read.
enum OpenValue {
    Array(Vec<serde_json::Value>),
    Object(serde_json::Map<String, serde_json::Value>, String),
}

/// [`parsed`]'s document for a body [`valid`] holds to be one, built without recursion from the
/// walk [`Skip`] makes, each number as 1.5.5's reader read it ([`number`]), each
/// string decoded; a repeated key keeps its last value, as serde's map does. Only reached where
/// serde refuses a number 1.5.5's reader took.
fn built(body: &[u8]) -> Option<serde_json::Value> {
    use serde_json::Value;
    let mut at = Skip::lenient(body);
    let mut open: Vec<OpenValue> = Vec::new();
    loop {
        let mut value = match at.next_non_space()? {
            b'{' => match at.next_non_space()? {
                b'}' => Value::Object(serde_json::Map::new()),
                b'"' => {
                    let key = at.key()?;
                    open.push(OpenValue::Object(serde_json::Map::new(), key));
                    continue;
                }
                _ => return None,
            },
            b'[' => {
                at.eat_spaces();
                if at.peek()? == b']' {
                    at.at += 1;
                    Value::Array(Vec::new())
                } else {
                    open.push(OpenValue::Array(Vec::new()));
                    continue;
                }
            }
            b'"' => Value::String(at.string_value()?),
            first @ (b'-' | b'0'..=b'9') => at.number_value(first)?,
            b't' => at.skip_literal(b"rue").ok().map(|()| Value::Bool(true))?,
            b'f' => at.skip_literal(b"alse").ok().map(|()| Value::Bool(false))?,
            b'n' => at.skip_literal(b"ull").ok().map(|()| Value::Null)?,
            _ => return None,
        };
        // The value ends every container it is the last of.
        loop {
            let next = match open.last_mut() {
                None => return Some(value),
                Some(OpenValue::Array(items)) => {
                    items.push(value);
                    at.next_non_space()?
                }
                Some(OpenValue::Object(members, key)) => {
                    members.insert(core::mem::take(key), value);
                    at.next_non_space()?
                }
            };
            let in_array = matches!(open.last()?, OpenValue::Array(_));
            match (next, in_array) {
                (b',', true) => break,
                (b',', false) => {
                    if at.next_non_space()? != b'"' {
                        return None;
                    }
                    let next_key = at.key()?;
                    if let Some(OpenValue::Object(_, key)) = open.last_mut() {
                        *key = next_key;
                    }
                    break;
                }
                (b']', true) | (b'}', false) => {
                    value = match open.pop()? {
                        OpenValue::Array(items) => Value::Array(items),
                        OpenValue::Object(members, _) => Value::Object(members),
                    };
                }
                _ => return None,
            }
        }
    }
}

/// The unknown-model refusal's words before the echoed name.
const UNKNOWN_HEAD: &str = "The model '";

/// The unknown-model refusal's words after the echoed name.
const UNKNOWN_TAIL: &str = "' does not exist or you do not have access to it.";

/// The most bytes of a requested model's name an unknown-model refusal echoes: every byte the
/// refusal's words may run (`busbar_contract::abi::plane::MAX_REFUSAL_TEXT`, past which the `arrive`
/// is a FAULT) but the sentence's own. Every name up to it is echoed whole; only a name the refusal could not carry is cut, on a character boundary, and marked
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

/// The top-level keys decoded while a body is validated: `model`, `stream`, `stream_options`,
/// `system` and the array-stream shim key. Their values are decoded whole (so a lone surrogate
/// escape in one refuses the body); every other value is only scanned.
const HEAD_KEYS: &[&str] = &[
    "__busbar_gemini_json_array",
    "model",
    "stream",
    "stream_options",
    "system",
];

/// The deepest value of [`HEAD_KEYS`] decoded (sonic-rs 0.5.8 `MAX_ALLOWED_DEPTH`): its deserializer starts with `u8::MAX` (255) levels,
/// each object or array it builds takes one and the 255th refuses `RecursionLimitExceeded`; the root
/// object takes the first, so a head value may nest 253 deep. 1.5.5's depth scan ([`busbar_contract::spans::exceeds_max_depth`])
/// holds a body to 128 first, but a skipped value whose `\u` escape swallows the closing quote
/// ([`Skip::skip_escape`]) leaves the scan reading the rest of the body as a string, so a head value
/// after it reaches its decode however deep it nests. Held to this, the decode's recursion is
/// bounded where 1.5.5's was.
const HEAD_VALUE_DEPTH: usize = 253;

/// The body as ingress reads it: at most [`MAX_PARSE_DEPTH`] deep by the depth scan, UTF-8
/// throughout, then one validating parse that builds no document (trailing bytes, a bad number, a
/// bad escape or a truncated document refuse), decoding the top-level keys, the values of
/// [`HEAD_KEYS`] and a root scalar (so a root number that overflows to infinity refuses) and only
/// skipping every other value, as sonic-rs 0.5.8 skips it ([`Skip`]): a `\u` escape inside a skipped value is not checked for its
/// hex digits, a skipped number is not checked for its range and a skipped value has no depth bound.
/// Answers the top-level `model`'s value, the last one where it repeats; `None` when the body
/// names none or is no object.
///
/// # Errors
///
/// [`Unrouted::Unparsable`]: a 400.
pub fn readable(body: &[u8]) -> Result<Option<serde_json::Value>, Unrouted> {
    if busbar_contract::spans::exceeds_max_depth(body, MAX_PARSE_DEPTH) {
        return Err(Unrouted::Unparsable);
    }
    core::str::from_utf8(body).map_err(|_| Unrouted::Unparsable)?;
    let mut skip = Skip::lenient(body);
    let model = skip.top_level().map_err(|()| Unrouted::Unparsable)?;
    if skip.next_non_space().is_some() {
        return Err(Unrouted::Unparsable);
    }
    Ok(model)
}

/// Whether `span` is one value as 1.5.5 DECODED it (a top-level key, a value of [`HEAD_KEYS`], a
/// root scalar), read strictly ([`Skip::strict`]): every escape whole, every surrogate paired and
/// every number one 1.5.5's reader took ([`number`]). The span was skipped first
/// ([`Skip`]), its depth bounded by the caller.
fn decodes(span: &[u8]) -> bool {
    let mut strict = Skip::strict(span);
    strict.skip_one().is_ok() && strict.next_non_space().is_none()
}

/// [`decodes`], and the value built. A number serde reads as out of range where 1.5.5's reader did
/// not (the exponent cap of [`number`]) is built as `null`: only the `model` is kept,
/// and a `model` that is no string is none ([`Unrouted::NotAName`]) either way.
fn decoded(span: &[u8]) -> Result<serde_json::Value, ()> {
    if !decodes(span) {
        return Err(());
    }
    let mut de = serde_json::Deserializer::from_slice(span);
    de.disable_recursion_limit();
    Ok(serde::Deserialize::deserialize(&mut de).unwrap_or(serde_json::Value::Null))
}

/// sonic-rs 0.5.8's CHECKED SKIP (`Parser::skip_one(true)`, what `serde::de::IgnoredAny` ran in
/// 1.5.5's head parse), over a body: structure, numbers, literals, control characters and the
/// simple escapes are validated, and a `\u` escape is skipped WITHOUT looking at its hex
/// (`skip_escaped_chars`: after a backslash it consumes the next byte, and for `u` four more, which
/// may be anything but the buffer's end). So a value 1.5.5 only skipped accepts `\uZZZZ` and a lone
/// surrogate, while one it decoded refuses them ([`decoded`]). sonic's skip has no depth bound; this
/// one keeps its open containers on the heap, not the stack, so no depth overflows it.
///
/// STRICT ([`Skip::strict`]), the same walk reads a value as 1.5.5 DECODED it: each string is also
/// decoded (an escape whole, a surrogate paired) and each number read by 1.5.5's own reader,
/// [`number`], which refuses one that overflows to infinity as 1.5.5 refused it.
struct Skip<'b> {
    bytes: &'b [u8],
    at: usize,
    strict: bool,
}

/// The value 1.5.5's reader read `bytes` as (one JSON number, held to the JSON grammar): an
/// unsigned or a negative integer, or a finite float; `None` where it refused the number
/// (`FloatMustBeFinite`, the parse 400). The reader is sonic-number's own `parse_number`, at the
/// version busbar's lock carries (0.1.3), which reads every number as 0.1.2 did (its exponent cap at
/// 1000, its 19 significant digits): `tests/number_reader.rs` holds it to the verdicts of the port
/// of 0.1.2 this plane carried before.
fn number(bytes: &[u8]) -> Option<sonic_number::ParserNumber> {
    let negative = bytes.first() == Some(&b'-');
    let mut at = usize::from(negative);
    sonic_number::parse_number(bytes, &mut at, negative).ok()
}

/// The bytes a backslash may be followed by (sonic-rs 0.5.8 `ESCAPED_TAB`), `u` apart.
const SIMPLE_ESCAPES: &[u8] = b"\"\\/bfnrt";

impl<'b> Skip<'b> {
    /// sonic's checked skip over `bytes`.
    const fn lenient(bytes: &'b [u8]) -> Self {
        Skip {
            bytes,
            at: 0,
            strict: false,
        }
    }

    /// The walk of a value 1.5.5 decoded, over `bytes`.
    const fn strict(bytes: &'b [u8]) -> Self {
        Skip {
            bytes,
            at: 0,
            strict: true,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.at += 1;
        Some(b)
    }

    fn eat_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// Past any whitespace, the next byte, consumed.
    fn next_non_space(&mut self) -> Option<u8> {
        self.eat_spaces();
        self.bump()
    }

    /// The root value: an object's keys and head values decoded, an array's elements skipped, a
    /// root scalar (string, number or literal) decoded. Answers the `model` value.
    fn top_level(&mut self) -> Result<Option<serde_json::Value>, ()> {
        self.eat_spaces();
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                self.head_object()
            }
            Some(b'[') => {
                self.skip_one()?;
                Ok(None)
            }
            _ => {
                let start = self.at;
                self.skip_one()?;
                decodes(&self.bytes[start..self.at])
                    .then_some(None)
                    .ok_or(())
            }
        }
    }

    /// The root object's members, its `{` consumed.
    fn head_object(&mut self) -> Result<Option<serde_json::Value>, ()> {
        let mut model = None;
        match self.next_non_space() {
            Some(b'}') => return Ok(None),
            Some(b'"') => {}
            _ => return Err(()),
        }
        loop {
            let key_start = self.at - 1;
            self.skip_string()?;
            let serde_json::Value::String(key) = decoded(&self.bytes[key_start..self.at])? else {
                return Err(());
            };
            if self.next_non_space() != Some(b':') {
                return Err(());
            }
            self.eat_spaces();
            let start = self.at;
            let deepest = self.skip_one()?;
            if HEAD_KEYS.contains(&key.as_str()) {
                if deepest > HEAD_VALUE_DEPTH {
                    return Err(());
                }
                let span = &self.bytes[start..self.at];
                if key == "model" {
                    model = Some(decoded(span)?);
                } else if !decodes(span) {
                    return Err(());
                }
            }
            match self.next_non_space() {
                Some(b'}') => return Ok(model),
                Some(b',') if self.next_non_space() == Some(b'"') => {}
                _ => return Err(()),
            }
        }
    }

    /// One value skipped (`skip_one(true)`, with sonic's `skip_object` and `skip_array`), without
    /// recursion: the containers still open are kept innermost last (`true` an object). Answers the
    /// deepest nesting the value reached: `0` for a scalar, `1` for `[]`.
    fn skip_one(&mut self) -> Result<usize, ()> {
        let mut open: Vec<bool> = Vec::new();
        let mut deepest = 0;
        loop {
            match self.next_non_space().ok_or(())? {
                first @ (b'-' | b'0'..=b'9') => self.skip_number(first)?,
                b'"' => self.skip_string()?,
                b't' => self.skip_literal(b"rue")?,
                b'f' => self.skip_literal(b"alse")?,
                b'n' => self.skip_literal(b"ull")?,
                b'{' => {
                    deepest = deepest.max(open.len() + 1);
                    match self.next_non_space() {
                        Some(b'}') => {}
                        Some(b'"') => {
                            open.push(true);
                            self.skip_key()?;
                            continue;
                        }
                        _ => return Err(()),
                    }
                }
                b'[' => {
                    deepest = deepest.max(open.len() + 1);
                    self.eat_spaces();
                    match self.peek().ok_or(())? {
                        b']' => self.at += 1,
                        _ => {
                            open.push(false);
                            continue;
                        }
                    }
                }
                _ => return Err(()),
            }
            // A value ended: close every container it was the last of, up to one it is not.
            loop {
                let Some(&object) = open.last() else {
                    return Ok(deepest);
                };
                match (self.next_non_space(), object) {
                    (Some(b','), false) => break,
                    (Some(b','), true) if self.next_non_space() == Some(b'"') => {
                        self.skip_key()?;
                        break;
                    }
                    (Some(b']'), false) | (Some(b'}'), true) => {
                        open.pop();
                    }
                    _ => return Err(()),
                }
            }
        }
    }

    /// [`built`]'s string, its opening quote consumed: decoded.
    fn string_value(&mut self) -> Option<String> {
        let start = self.at - 1;
        self.skip_string_token().ok()?;
        serde_json::from_slice(&self.bytes[start..self.at]).ok()
    }

    /// [`built`]'s key and its colon, the key's opening quote consumed.
    fn key(&mut self) -> Option<String> {
        let key = self.string_value()?;
        (self.next_non_space()? == b':').then_some(key)
    }

    /// [`built`]'s number, its first byte consumed: as 1.5.5's reader read it.
    fn number_value(&mut self, first: u8) -> Option<serde_json::Value> {
        use sonic_number::ParserNumber;
        let start = self.at - 1;
        self.skip_number_token(first).ok()?;
        Some(match number(&self.bytes[start..self.at])? {
            ParserNumber::Unsigned(n) => serde_json::Value::from(n),
            ParserNumber::Signed(n) => serde_json::Value::from(n),
            ParserNumber::Float(f) => serde_json::Value::Number(serde_json::Number::from_f64(f)?),
        })
    }

    /// A skipped object's key and its colon, the key's opening quote consumed.
    fn skip_key(&mut self) -> Result<(), ()> {
        self.skip_string()?;
        if self.next_non_space() == Some(b':') {
            Ok(())
        } else {
            Err(())
        }
    }

    fn skip_literal(&mut self, rest: &[u8]) -> Result<(), ()> {
        let end = self.at.checked_add(rest.len()).ok_or(())?;
        if self.bytes.get(self.at..end) != Some(rest) {
            return Err(());
        }
        self.at = end;
        Ok(())
    }

    fn skip_digit(&mut self) -> Result<u8, ()> {
        self.bump().filter(u8::is_ascii_digit).ok_or(())
    }

    fn skip_digits(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.at += 1;
        }
    }

    fn skip_exponent(&mut self) -> Result<(), ()> {
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.at += 1;
        }
        self.skip_digit()?;
        self.skip_digits();
        Ok(())
    }

    /// `do_skip_number`: an optional `-`, a digit (no leading zero before another), at most one
    /// fraction (a digit required) and an exponent (a digit required). Its range is not checked.
    fn skip_number(&mut self, first: u8) -> Result<(), ()> {
        let start = self.at - 1;
        self.skip_number_token(first)?;
        if self.strict && number(&self.bytes[start..self.at]).is_none() {
            return Err(());
        }
        Ok(())
    }

    /// One number's grammar, its first byte consumed.
    fn skip_number_token(&mut self, mut first: u8) -> Result<(), ()> {
        if first == b'-' {
            first = self.skip_digit()?;
        }
        let second = self.peek();
        if first == b'0' && second.is_some_and(|b| b.is_ascii_digit()) {
            return Err(());
        }
        let mut fraction = false;
        match second {
            Some(b'0'..=b'9') => self.at += 1,
            Some(b'.') => {
                fraction = true;
                self.at += 1;
                self.skip_digit()?;
            }
            Some(b'e' | b'E') => {
                self.at += 1;
                return self.skip_exponent();
            }
            _ => return Ok(()),
        }
        self.skip_digits();
        match self.peek() {
            Some(b'.') if !fraction => {
                self.at += 1;
                self.skip_digit()?;
                self.skip_digits();
                if matches!(self.peek(), Some(b'e' | b'E')) {
                    self.at += 1;
                    return self.skip_exponent();
                }
                Ok(())
            }
            Some(b'e' | b'E') => {
                self.at += 1;
                self.skip_exponent()
            }
            _ => Ok(()),
        }
    }

    /// `skip_string`, the opening quote consumed: a control character refuses, a backslash is
    /// followed by [`Self::skip_escape`].
    fn skip_string(&mut self) -> Result<(), ()> {
        let start = self.at - 1;
        self.skip_string_token()?;
        if self.strict && !string_decodes(&self.bytes[start..self.at]) {
            return Err(());
        }
        Ok(())
    }

    /// One string's bytes, its opening quote consumed.
    fn skip_string_token(&mut self) -> Result<(), ()> {
        loop {
            match self.bump().ok_or(())? {
                b'\\' => self.skip_escape()?,
                b'"' => return Ok(()),
                0..=0x1f => return Err(()),
                _ => {}
            }
        }
    }

    /// `skip_escaped_chars`, the backslash consumed. `u` needs six bytes left (itself, four more and
    /// the byte after) and eats five, unchecked; any other byte is eaten and must be a simple
    /// escape.
    fn skip_escape(&mut self) -> Result<(), ()> {
        match self.peek().ok_or(())? {
            b'u' if self.bytes.len() - self.at < 6 => Err(()),
            b'u' => {
                self.at += 5;
                Ok(())
            }
            c => {
                self.at += 1;
                SIMPLE_ESCAPES.contains(&c).then_some(()).ok_or(())
            }
        }
    }
}

/// What a read of the body ([`readable`], once) names as its model: the model of a
/// read already made.
///
/// # Errors
///
/// [`Unrouted::Unparsable`], [`Unrouted::NotAName`], [`Unrouted::EmptyModel`].
pub fn named_in<'a>(
    read: &Result<Option<serde_json::Value>, Unrouted>,
) -> Result<Option<std::borrow::Cow<'a, str>>, Unrouted> {
    match read {
        Err(unrouted) => Err(unrouted.clone()),
        Ok(None) => Ok(None),
        Ok(Some(serde_json::Value::String(name))) if name.is_empty() => Err(Unrouted::EmptyModel),
        Ok(Some(serde_json::Value::String(name))) => {
            Ok(Some(std::borrow::Cow::Owned(name.clone())))
        }
        Ok(Some(_)) => Err(Unrouted::NotAName),
    }
}

/// The model a claimed request routes over among the `models` its claim's dialect `serves` (the
/// generation's configured names, each with what this plane keeps of it): the one it names, else
/// the only one of that dialect configured. Answers the configured name and its entry. A model of
/// another dialect is this dialect's unknown model.
///
/// # Errors
///
/// [`Unrouted`]: several configured and none named, a `model` that is not a string, or a name none
/// of the dialect's models has.
#[cfg(test)]
pub fn route_model<'m, V>(
    body: &[u8],
    models: &'m std::collections::BTreeMap<String, V>,
    serves: impl Fn(&V) -> bool,
) -> Result<(&'m str, &'m V), Unrouted> {
    route_named(body, models, serves).map(|(name, entry, _)| (name, entry))
}

/// [`route_model`], also answering the `model` the body named (the head's read, once), for a caller
/// that needs it again ([`spliceable_named`]) without reading the body a second time.
///
/// # Errors
///
/// As [`route_model`].
#[allow(clippy::type_complexity)]
pub fn route_named<'m, 'b, V>(
    body: &'b [u8],
    models: &'m std::collections::BTreeMap<String, V>,
    serves: impl Fn(&V) -> bool,
) -> Result<(&'m str, &'m V, Option<std::borrow::Cow<'b, str>>), Unrouted> {
    route_read(&readable(body), models, serves)
}

/// [`route_named`] over a read of the body already made ([`readable`], once).
///
/// # Errors
///
/// As [`route_model`].
#[allow(clippy::type_complexity)]
pub fn route_read<'m, 'b, V>(
    read: &Result<Option<serde_json::Value>, Unrouted>,
    models: &'m std::collections::BTreeMap<String, V>,
    serves: impl Fn(&V) -> bool,
) -> Result<(&'m str, &'m V, Option<std::borrow::Cow<'b, str>>), Unrouted> {
    let requested = named_in(read)?;
    match &requested {
        Some(name) => models
            .get_key_value(name.as_ref())
            .filter(|(_, v)| serves(v))
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| Unrouted::Unknown(echoed(name))),
        None => {
            let mut all = models.iter().filter(|(_, v)| serves(v));
            match (all.next(), all.next()) {
                (Some((k, v)), None) => Ok((k.as_str(), v)),
                _ => Err(Unrouted::Unnamed),
            }
        }
    }
    .map(|(name, entry)| (name, entry, requested))
}

/// The `upstream_model` splice: `body` with its top-level `model` value replaced by `upstream`, when
/// the body names a model and it differs from `upstream`; `None` when the body passes through
/// unchanged. Every byte outside the value's span is kept: member order, whitespace and the rest of
/// the document are the caller's.
#[must_use]
pub fn spliced(body: &[u8], upstream: &str) -> Option<Vec<u8>> {
    // The body was held to the document ingress reads at `arrive`: it is not judged again here.
    let span = match busbar_contract::spans::resolve_pointer(body, codec::PTR_MODEL) {
        busbar_contract::spans::Resolved::Found(span) => span,
        busbar_contract::spans::Resolved::Missing => return None,
        busbar_contract::spans::Resolved::Malformed
        | busbar_contract::spans::Resolved::NeedMore => {
            // The scanner cannot walk the body: the model is set in the parsed document, every
            // other member keeping its value.
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

/// Whether [`spliced`] can set `upstream` in `body` where it must: a body naming a model other
/// than `upstream` is rewritten on its hop, which materializes the whole document; one whose
/// document would not parse (a string its head never decoded but the document parse refuses) is
/// answered 500 "We received an unexpected internal error. Please try again." `arrive` answers it
/// so, never relaying the caller's model unspliced. A body naming no model, or `upstream` itself,
/// needs no rewrite.
#[cfg(test)]
#[must_use]
pub fn spliceable(body: &[u8], upstream: &str) -> bool {
    match readable(body) {
        Ok(Some(serde_json::Value::String(named))) => {
            spliceable_named(body, Some(&named), upstream)
        }
        _ => true,
    }
}

/// [`spliceable`] for a caller that already holds the `model` the body named (the head's read, as
/// [`route_named`] answers it): the document is only VALIDATED ([`valid`]), never built.
#[must_use]
pub fn spliceable_named(body: &[u8], named: Option<&str>, upstream: &str) -> bool {
    match named {
        Some(named) if named != upstream => valid(body),
        _ => true,
    }
}

/// Whether the body's whole document parses, as the DOM parse reads it (strings decoded, so a
/// bad escape or a lone surrogate refuses; numbers read by [`number`]), by a walk that builds no tree ([`Skip::strict`]).
#[must_use]
pub fn valid(body: &[u8]) -> bool {
    if busbar_contract::spans::exceeds_max_depth(body, MAX_PARSE_DEPTH)
        || core::str::from_utf8(body).is_err()
    {
        return false;
    }
    decodes(body)
}

/// Whether `span`, one string with its quotes, decodes as 1.5.5 decoded a string: every escape
/// whole and every surrogate paired. Borrowed where it has no escape: nothing is built.
fn string_decodes(span: &[u8]) -> bool {
    let mut de = serde_json::Deserializer::from_slice(span);
    <Valid as serde::Deserialize>::deserialize(&mut de).is_ok() && de.end().is_ok()
}

/// [`string_decodes`]'s visit: the value read, nothing kept.
struct Valid;

impl<'de> serde::Deserialize<'de> for Valid {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ValidVisitor)
    }
}

struct ValidVisitor;

impl<'de> serde::de::Visitor<'de> for ValidVisitor {
    type Value = Valid;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a JSON document")
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Valid, E> {
        Ok(Valid)
    }

    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Valid, E> {
        Ok(Valid)
    }

    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Valid, E> {
        Ok(Valid)
    }

    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Valid, E> {
        Ok(Valid)
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Valid, E> {
        Ok(Valid)
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Valid, E> {
        Ok(Valid)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Valid, A::Error> {
        while seq.next_element::<Valid>()?.is_some() {}
        Ok(Valid)
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Valid, A::Error> {
        while map.next_key::<Valid>()?.is_some() {
            map.next_value::<Valid>()?;
        }
        Ok(Valid)
    }
}

/// The words for a request whose document would not parse where it had to be rewritten for its
/// hop, at 500.
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

/// Project one request of `dialect` for the hooks: the model it routes over among the `routes` of
/// that dialect ([`route_model`]; `dialect_of` reads a route's dialect), the dialect's name and the
/// size of the document. The document itself is the view's one turn and its body, as the caller
/// sent it (or as a rewrite left it); the plane reads nothing of it for itself beyond its top-level
/// `model`, and mints no fact from it.
#[cfg(test)]
#[must_use]
pub fn project_hook<'r, V>(
    request: &[u8],
    dialect: u32,
    routes: &'r std::collections::BTreeMap<String, V>,
    dialect_of: impl Fn(&V) -> u32,
) -> Projected<'r> {
    project_read(request, &readable(request), dialect, routes, dialect_of)
}

/// [`project`] over a read of the request already made ([`readable`], once).
#[must_use]
pub fn project_read<'r, V>(
    request: &[u8],
    read: &Result<Option<serde_json::Value>, Unrouted>,
    dialect: u32,
    routes: &'r std::collections::BTreeMap<String, V>,
    dialect_of: impl Fn(&V) -> u32,
) -> Projected<'r> {
    let entry =
        route_read(read, routes, |r| dialect_of(r) == dialect).map_or("", |(name, _, _)| name);
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

/// The document a request-stage hook's rewrite names (the rewrite JSON, `{"messages": [...],
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

/// A rewrite that cannot be applied, and how the plane answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotApplied {
    /// The document names another model: REFUSED, the hook's failure (its `on_error`).
    Model(ModelRewritten),
    /// The rewrite has a document and the body's whole document does not parse (a member the
    /// head only skipped, carrying a bad escape or a lone surrogate): FAULT, which the kernel always
    /// answers 500 `request rewrite could not be applied`, whatever the hook's `on_error`, the body
    /// never forwarded.
    Fault,
}

/// A rewrite applied in the dialect: the rewrite's document ([`rewrite_document`]) replaces the
/// request wholesale. A member the document omits is dropped, so a hook redacts by omission; a
/// member it states is set to its value, the caller's own bytes kept for a value it leaves as it
/// was; a member the caller did not send is added after the caller's. The top-level `model` alone
/// is not the document's: it stays the caller's whether or not the document repeats it, and a
/// document naming another model is refused ([`NotApplied::Model`]), never a silent switch of
/// provider. `routed` is the model entry the request is routed over.
///
/// `Ok(None)` when the rewrite names no document, the body is not one JSON object, or the document
/// is the request as it stands: the request goes on byte for byte as it was (a rewrite that
/// cannot be applied leaves the request unmodified). A rewrite WITH a document over a body whose
/// whole document does not parse is never applied wholesale and never skipped: it is
/// [`NotApplied::Fault`].
///
/// # Errors
///
/// [`NotApplied::Model`]: the document names a model other than `routed`; [`NotApplied::Fault`]:
/// the body's document does not parse, or the head could not read the body at all.
#[cfg(test)]
pub fn rewritten(body: &[u8], rewrite: &[u8], routed: &str) -> Result<Option<Vec<u8>>, NotApplied> {
    rewritten_read(body, &readable(body), rewrite, routed)
}

/// [`rewritten`] over a read of the body already made ([`readable`], once).
///
/// # Errors
///
/// As [`rewritten`].
pub fn rewritten_read(
    body: &[u8],
    read: &Result<Option<serde_json::Value>, Unrouted>,
    rewrite: &[u8],
    routed: &str,
) -> Result<Option<Vec<u8>>, NotApplied> {
    const MODEL: &str = "model";
    // The caller's `model`, as `arrive` read it.
    // A body the head could not read is no request to rewrite: FAULT, the safer answer.
    let Ok(caller_model) = read else {
        return Err(NotApplied::Fault);
    };
    // The body's whole document comes first, before anything the hook answered is read. A body
    // whose document does not parse (an array too) is a FAULT whatever the hook's rewrite says or
    // whether the rewrite can be read at all (a hook that echoes the bad member back answers a
    // rewrite that does not read either): never "does not apply", never applied over it, never
    // forwarded. Read to the parse depth (the request was held to it at `arrive`), not serde's one
    // short.
    let current = match parsed(body) {
        Ok(serde_json::Value::Object(map)) => Some(map),
        Ok(_) => None,
        Err(_) => return Err(NotApplied::Fault),
    };
    let Some(mut document) = rewrite_document(rewrite) else {
        return Ok(None);
    };
    if let Some(named) = document.remove(MODEL) {
        let same = named.as_str() == Some(routed) || caller_model.as_ref() == Some(&named);
        if !same {
            return Err(NotApplied::Model(ModelRewritten {
                named: named
                    .as_str()
                    .map_or_else(|| named.to_string(), str::to_string),
            }));
        }
    }
    let Some(current) = current else {
        return Ok(None);
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
        kept.push((span.start, member_bytes(key, &bytes)));
    }
    kept.sort_by_key(|(at, _)| *at);
    let mut members: Vec<Vec<u8>> = kept.into_iter().map(|(_, m)| m).collect();
    for (key, value) in document {
        if !current.contains_key(key) {
            members.push(member_bytes(key, value.to_string().as_bytes()));
        }
    }
    let mut out = Vec::with_capacity(body.len());
    out.push(b'{');
    out.extend_from_slice(&members.join(&b','));
    out.push(b'}');
    Some(out)
}

/// One member, `"key":value`, `value` as written.
fn member_bytes(key: &str, value: &[u8]) -> Vec<u8> {
    let mut m = serde_json::Value::String(key.to_string())
        .to_string()
        .into_bytes();
    m.push(b':');
    m.extend_from_slice(value);
    m
}

/// A claimed request, classified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arrived {
    /// The operation, by name.
    pub op: OpClassId,
    /// Its index in [`tail::OP_CLASSES`].
    pub op_class: u32,
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
pub struct FarEndRequest {
    /// The verb, explicit.
    pub verb: &'static str,
    /// The target, explicit.
    pub target: &'static str,
    /// The dialect fields. The kernel adds the auth fields for [`FarEndRequest::auth`].
    pub fields: [(&'static str, &'static [u8]); 1],
    /// The auth scheme the kernel decorates the request under.
    pub auth: &'static str,
}

/// Answer an ATTEMPT piece of a unit of `dialect`: the same request on every attempt, whichever
/// member the kernel picked, at the dialect's own path. Neither dialect names a provider on the
/// wire, so the member changes nothing in the request.
#[must_use]
pub fn attempt_request(dialect: u32) -> FarEndRequest {
    let target = tail::CLAIMS
        .iter()
        .find(|(_, _, d)| *d == dialect)
        .map_or(ops::PATH_SYSTEMONE, |(_, t, _)| t);
    FarEndRequest {
        verb: "POST",
        target,
        fields: [(FIELD_CONTENT_TYPE, CONTENT_TYPE_JSON)],
        auth: EGRESS_SCHEME,
    }
}

/// Where a count the plane reports comes from: the far end's own counts are kept apart from the
/// plane's floor; both bill.
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

/// A usage member the far end's success answer carries but that is not a whole count (or a
/// required count it does not carry): located, never dropped, never a silent 0. The
/// door bills the plane's floor for it ([`FarEndReading::floored`]) and says so in one log line.
/// Names the member's pointer.
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
/// Usage pointers are dialect data.
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
/// answer of any size is read in the same few bytes, with no allocation and no blocking on the hot
/// path, and the pieces may split the answer anywhere.
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

    /// What the answer bills, read at the last piece: a non-success bills nothing; a success bills one `decision` (the plane's floor,
    /// [`Source::Floor`]), or the whole count its
    /// `/usage/units` reports (the hosted provider's count), its `/usage/input_tokens`
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
        Ok(Self::billed(whole))
    }

    /// What a success whose usage could not be read whole bills ([`Self::counts`] answered
    /// [`UnreadableUsage`]; spend is always attributed, and a unit that succeeded is never
    /// failed): the plane's floor for an unreadable decision count (one
    /// decision, [`Source::Floor`]), every token count it does state whole, and the request fee. A
    /// non-success bills nothing.
    #[must_use]
    pub fn floored(&self, status: Option<u32>) -> Counts {
        if !self.success(status) {
            return [None; 4];
        }
        let (_, members) = self.settled();
        let mut whole = [None; 3];
        for (i, _) in self.usage().counted.iter().enumerate() {
            whole[i] = match members[i] {
                Member::Whole(n) => Some(n),
                Member::Absent | Member::Unreadable => None,
            };
        }
        Self::billed(whole)
    }

    /// What an answer the far end did not finish bills (its last piece flagged `PIECE_CUT`: its
    /// connection failed, or the send's deadline passed, after its head). It is no delivered
    /// reply: it bills no floor and no request fee (a failed unit owes none), only each count the
    /// far end reported whole before the cut (a count the cut ends mid-number is none), and
    /// nothing when what it said before the cut is no success.
    #[must_use]
    pub fn cut(&self, status: Option<u32>) -> Counts {
        if !self.success(status) {
            return [None; 4];
        }
        // The members as read: a scalar the cut ended in is not settled into a count.
        let reported = |class: u32, member: Member| match member {
            Member::Whole(amount) => Some(Units {
                class,
                source: Source::Reported,
                amount,
            }),
            Member::Absent | Member::Unreadable => None,
        };
        [
            reported(tail::CLASS_DECISION_INDEX, self.members[0]),
            reported(tail::CLASS_INPUT_TOKENS_INDEX, self.members[1]),
            reported(tail::CLASS_OUTPUT_TOKENS_INDEX, self.members[2]),
            None,
        ]
    }

    /// The counts of a success whose whole counts are `whole` (decision, input and output tokens).
    fn billed(whole: [Option<u64>; 3]) -> Counts {
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
        [
            Some(decisions),
            whole[1].map(|n| reported(tail::CLASS_INPUT_TOKENS_INDEX, n)),
            whole[2].map(|n| reported(tail::CLASS_OUTPUT_TOKENS_INDEX, n)),
            Some(reported(tail::CLASS_FEE_INDEX, 1)),
        ]
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

/// THE CALLER FIELDS THE FAR END NEVER RECEIVES, besides the credential carriers: the ones the
/// plane writes itself (the document type) and the ones naming the hop (`host`). Compared without
/// case. Hop-by-hop fields and the body framing never reach the plane (the framer drops them).
///
/// The credential carriers are the contract's one governed set,
/// [`NEVER_KEPT`](busbar_contract::abi::host::conn::connector::NEVER_KEPT) (`authorization`,
/// `proxy-authorization`, `cookie`, `x-api-key`, `api-key`, `x-goog-api-key`,
/// `x-amz-security-token`, ...): the far end is presented the member's credential by the kernel,
/// never the caller's.
pub const NOT_RELAYED: &[&str] = &[FIELD_CONTENT_TYPE, "host"];

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
        .chain(busbar_contract::abi::host::conn::connector::NEVER_KEPT)
        .any(|n| name.eq_ignore_ascii_case(n.as_bytes()))
}

/// The caller's fields the far end receives, in the order the caller sent them, a repeated name
/// keeping every value: every field the caller sent but [`NOT_RELAYED`]'s and the credential
/// carriers' (a same-dialect relay passes every header but the governed set). Names are
/// lowercased, as the wire compares them. Kept in one buffer sized once.
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
