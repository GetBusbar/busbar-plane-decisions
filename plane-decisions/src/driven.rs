//! This plane's answers to the kernel plane driver, in this plane's own vocabulary.

use busbar_contract::ids::{MeterClassId, OpClassId};
use busbar_contract::Count;

use crate::codec::{
    self, error_body, CONTENT_TYPE_JSON, FIELD_CONTENT_TYPE, PTR_EXECUTION_STATES,
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

    /// The outbound auth style both dialects default to ([`DIALECT_AUTH`]) and the plane's one
    /// Need names ([`NEEDS`]): the operator's credential as a bearer, returned as a locator the
    /// plane never resolves.
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

    /// The op classes this plane serves, in index order: each dialect's decision call (see
    /// [`crate::ops`]).
    pub const OP_CLASSES: &[OpClassId] = &[ops::OP_SYSTEMONE, ops::OP_EVALUATE];

    /// The billable classes, in index order, each with its family: the decision count, the fee
    /// unit (also a class: the tail check holds `fee_units ⊆ billable_classes`), and the far end's
    /// input and output tokens.
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
    /// far end answered a success, so a refused or failed unit owes none.
    pub const FEE_PER_REQUEST: &str = busbar_contract::plane::PER_REQUEST;

    /// The family of the fee unit's class: a count of 0 or 1.
    pub const FEE_FAMILY: &str = "count";

    /// The index of the fee unit's class in [`BILLABLE_CLASSES`].
    pub const CLASS_FEE_INDEX: u32 = 1;

    /// The fee units this plane counts.
    pub const FEE_UNITS: &[&str] = &[FEE_PER_REQUEST];

    /// The connection needs, `(transport, auth)`, for the far-end direction: its dialect's style, the
    /// one a member resolves to by default (a member is bound on the need its resolved style names).
    pub const NEEDS: &[(&str, &str)] = &[(claims::TRANSPORT, OUTBOUND_STYLE)];

    /// The claims a generation snapshot may publish, `(verb, target, dialect)`: each dialect's one
    /// exact claim.
    pub const CLAIMS: &[(&str, &str, u32)] = &[
        ("POST", ops::PATH_SYSTEMONE, DIALECT_JEV),
        ("POST", ops::PATH_EVALUATE, DIALECT_NANOJEV),
    ];
}

/// The claims a generation's snapshot publishes, given the dialect of each model it configures:
/// only the routes of a dialect some model uses.
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
    /// The request names no model and the generation configures several of its dialect.
    Unnamed,
    /// The request names no model and the generation configures none of its dialect (a named one
    /// it does not configure is [`Unrouted::Unknown`]).
    NoModel,
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
            Self::Unknown(_) | Self::NoModel => 404,
        }
    }

    /// The words the caller reads, in the dialect's error shape ([`refusal_body`]).
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Unnamed => "the request names no model, and several decisions models are \
                              configured: name one in `model`"
                .to_string(),
            Self::NoModel => NO_MODEL.to_string(),
            Self::NotAName | Self::EmptyModel => MISSING_MODEL.to_string(),
            Self::Unknown(model) => format!("{UNKNOWN_HEAD}{model}{UNKNOWN_TAIL}"),
            Self::Unparsable => UNPARSABLE.to_string(),
        }
    }
}

/// The words a body that cannot be parsed is refused in, at 400.
pub const UNPARSABLE: &str = "We could not parse the JSON body of your request.";

/// The words a request with an empty `model` is refused in, at 400.
pub const MISSING_MODEL: &str = "Missing required parameter: 'model'.";

/// The words a request is refused in, at 404, when no configured model speaks its dialect.
pub const NO_MODEL: &str = "no decisions model is configured for this operation";

/// The parse depth a body is read to: a body nested deeper is refused before any document is
/// built, as one that does not parse.
pub const MAX_PARSE_DEPTH: usize = 128;

/// Materialize a bounded, valid request for a hook rewrite. Arbitrary-precision number storage
/// preserves numeric precision; untouched values retain their original spelling when spliced
/// from the caller's bytes by [`spliced_members`].
pub fn parsed(body: &[u8]) -> Result<serde_json::Value, Unrouted> {
    if !valid(body) {
        return Err(Unrouted::Unparsable);
    }
    // Build containers from their actual tokens. Deserializing an arbitrary object as Value
    // would mistake a literal `$serde_json::private::Number` key for AP number transport.
    Skip::new(body)
        .materialized()
        .map_err(|()| Unrouted::Unparsable)
}

/// The unknown-model refusal's words before the echoed name.
const UNKNOWN_HEAD: &str = "The model '";

/// The unknown-model refusal's words after the echoed name.
const UNKNOWN_TAIL: &str = "' does not exist or you do not have access to it.";

/// The most bytes of a requested model's name an unknown-model refusal echoes: every byte the
/// refusal's words may run (`busbar_contract::abi::plane::MAX_REFUSAL_TEXT`, past which the `arrive`
/// is a FAULT) but the sentence's own.
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

/// The request member a request names its model in, and routes by ([`codec::PTR_MODEL`]).
const MODEL: &str = "model";

/// Validate the bounded UTF-8 JSON document and read only its top-level `model`. Decision
/// content has uniform syntax rules, regardless of field names; numeric values remain opaque.
pub fn readable(body: &[u8]) -> Result<Option<serde_json::Value>, Unrouted> {
    if busbar_contract::spans::exceeds_max_depth(body, MAX_PARSE_DEPTH) {
        return Err(Unrouted::Unparsable);
    }
    core::str::from_utf8(body).map_err(|_| Unrouted::Unparsable)?;
    let mut skip = Skip::new(body);
    let model = skip.top_level().map_err(|()| Unrouted::Unparsable)?;
    if skip.next_non_space().is_some() {
        return Err(Unrouted::Unparsable);
    }
    Ok(model)
}

/// Whether `span` is one complete JSON value, with valid strings and numeric syntax.
fn decodes(span: &[u8]) -> bool {
    let mut strict = Skip::new(span);
    strict.skip_one().is_ok() && strict.next_non_space().is_none()
}

/// Decode only a routing key or model. Non-string models need no numeric interpretation:
/// they are represented as null and refused as `NotAName` by the routing check.
fn decoded(span: &[u8]) -> Result<serde_json::Value, ()> {
    if span.first() == Some(&b'"') {
        serde_json::from_slice::<String>(span)
            .map(serde_json::Value::String)
            .map_err(|_| ())
    } else {
        Ok(serde_json::Value::Null)
    }
}

/// An iterative JSON syntax walk. It validates every string, while keeping numbers as lexemes.
struct Skip<'b> {
    bytes: &'b [u8],
    at: usize,
}

/// Simple JSON escapes, apart from `u`.
const SIMPLE_ESCAPES: &[u8] = b"\"\\/bfnrt";

impl<'b> Skip<'b> {
    const fn new(bytes: &'b [u8]) -> Self {
        Self { bytes, at: 0 }
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

    /// Materialize a document already checked by `valid`, including its depth bound.
    /// Only scalar tokens reach serde's Value deserializer; object keys are always literal,
    /// including escaped spellings of serde's private markers. Duplicate keys keep the last value.
    fn materialized(&mut self) -> Result<serde_json::Value, ()> {
        use serde_json::{Map, Value};
        self.eat_spaces();
        match self.peek().ok_or(())? {
            b'{' => {
                self.at += 1;
                let mut fields = Map::new();
                match self.next_non_space() {
                    Some(b'}') => return Ok(Value::Object(fields)),
                    Some(b'"') => {}
                    _ => return Err(()),
                }
                loop {
                    let start = self.at - 1;
                    self.skip_string()?;
                    let key = serde_json::from_slice::<String>(&self.bytes[start..self.at])
                        .map_err(|_| ())?;
                    if self.next_non_space() != Some(b':') {
                        return Err(());
                    }
                    fields.insert(key, self.materialized()?);
                    match self.next_non_space() {
                        Some(b'}') => return Ok(Value::Object(fields)),
                        Some(b',') if self.next_non_space() == Some(b'"') => {}
                        _ => return Err(()),
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut values = Vec::new();
                self.eat_spaces();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Ok(Value::Array(values));
                }
                loop {
                    values.push(self.materialized()?);
                    match self.next_non_space() {
                        Some(b']') => return Ok(Value::Array(values)),
                        Some(b',') => {}
                        _ => return Err(()),
                    }
                }
            }
            _ => {
                let start = self.at;
                self.skip_one()?;
                serde_json::from_slice(&self.bytes[start..self.at]).map_err(|_| ())
            }
        }
    }

    /// Validate any root value; only an object's model is interpreted for routing.
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
            self.skip_one()?;
            if key == MODEL {
                model = Some(decoded(&self.bytes[start..self.at])?);
            }
            match self.next_non_space() {
                Some(b'}') => return Ok(model),
                Some(b',') if self.next_non_space() == Some(b'"') => {}
                _ => return Err(()),
            }
        }
    }

    /// One value validated without
    /// recursion: the containers still open are kept innermost last (`true` an object).
    fn skip_one(&mut self) -> Result<(), ()> {
        let mut open: Vec<bool> = Vec::new();
        loop {
            match self.next_non_space().ok_or(())? {
                first @ (b'-' | b'0'..=b'9') => self.skip_number_token(first)?,
                b'"' => self.skip_string()?,
                b't' => self.skip_literal(b"rue")?,
                b'f' => self.skip_literal(b"alse")?,
                b'n' => self.skip_literal(b"ull")?,
                b'{' => match self.next_non_space() {
                    Some(b'}') => {}
                    Some(b'"') => {
                        open.push(true);
                        self.skip_key()?;
                        continue;
                    }
                    _ => return Err(()),
                },
                b'[' => {
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

            loop {
                let Some(&object) = open.last() else {
                    return Ok(());
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
        if !string_decodes(&self.bytes[start..self.at]) {
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

    /// Consume a complete escape without letting invalid hex swallow structural bytes.
    fn skip_escape(&mut self) -> Result<(), ()> {
        match self.bump().ok_or(())? {
            b'u' => {
                for _ in 0..4 {
                    self.bump().filter(u8::is_ascii_hexdigit).ok_or(())?;
                }
                Ok(())
            }
            c => SIMPLE_ESCAPES.contains(&c).then_some(()).ok_or(()),
        }
    }
}

/// What a read of the body ([`readable`], once) names as its model: the model of a
/// read already made.
pub fn named_in(
    read: &Result<Option<serde_json::Value>, Unrouted>,
) -> Result<Option<String>, Unrouted> {
    match read {
        Err(unrouted) => Err(unrouted.clone()),
        Ok(None) => Ok(None),
        Ok(Some(serde_json::Value::String(name))) if name.is_empty() => Err(Unrouted::EmptyModel),
        Ok(Some(serde_json::Value::String(name))) => Ok(Some(name.clone())),
        Ok(Some(_)) => Err(Unrouted::NotAName),
    }
}

/// A routed request: the configured model's name and entry, and the `model` the body named.
pub type Routed<'m, V> = (&'m str, &'m V, Option<String>);

/// The model a claimed request routes over among the `models` its claim's dialect `serves` (the
/// generation's configured names, each with what this plane keeps of it): the one it names, else
/// the only one of that dialect configured.
pub fn route_named<'m, V>(
    body: &[u8],
    models: &'m std::collections::BTreeMap<String, V>,
    serves: impl Fn(&V) -> bool,
) -> Result<Routed<'m, V>, Unrouted> {
    route_read(&readable(body), models, serves)
}

/// [`route_named`] over a read of the body already made ([`readable`], once).
pub fn route_read<'m, V>(
    read: &Result<Option<serde_json::Value>, Unrouted>,
    models: &'m std::collections::BTreeMap<String, V>,
    serves: impl Fn(&V) -> bool,
) -> Result<Routed<'m, V>, Unrouted> {
    let requested = named_in(read)?;
    match &requested {
        Some(name) => models
            .get_key_value(name.as_str())
            .filter(|(_, v)| serves(v))
            .map(|(k, v)| (k.as_str(), v))
            .ok_or_else(|| Unrouted::Unknown(echoed(name))),
        None => {
            let mut all = models.iter().filter(|(_, v)| serves(v));
            match (all.next(), all.next()) {
                (Some((k, v)), None) => Ok((k.as_str(), v)),
                (None, _) => Err(Unrouted::NoModel),
                (Some(_), Some(_)) => Err(Unrouted::Unnamed),
            }
        }
    }
    .map(|(name, entry)| (name, entry, requested))
}

/// The `upstream_model` splice: `body` with its top-level `model` value replaced by `upstream`, when
/// the body names a model and it differs from `upstream`; `None` when the body passes through
/// unchanged.
#[must_use]
pub fn spliced(body: &[u8], upstream: &str) -> Option<Vec<u8>> {
    let span = match busbar_contract::spans::resolve_pointer(body, codec::PTR_MODEL) {
        busbar_contract::spans::Resolved::Found(span) => span,
        busbar_contract::spans::Resolved::Missing => return None,
        busbar_contract::spans::Resolved::Malformed
        | busbar_contract::spans::Resolved::NeedMore => {
            // Ingress already validated the bounded document. Locate its model without
            // converting numbers or reserializing any other value.
            busbar_contract::spans::members(body, 0)?
                .last(MODEL)?
                .value()
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

/// Validate a document when its route needs a model rename, without numeric conversion.
#[must_use]
pub fn spliceable_named(body: &[u8], named: Option<&str>, upstream: &str) -> bool {
    match named {
        // `named` came from a read [`readable`] accepted, so the body is UTF-8 and within the parse
        // depth already; validate its syntax without interpreting numeric content.
        Some(named) if named != upstream => decodes(body),
        _ => true,
    }
}

/// Whether the whole body is bounded UTF-8 JSON. Numeric syntax is validated without
/// conversion; strings must have valid escapes and paired surrogates.
#[must_use]
pub fn valid(body: &[u8]) -> bool {
    if busbar_contract::spans::exceeds_max_depth(body, MAX_PARSE_DEPTH)
        || core::str::from_utf8(body).is_err()
    {
        return false;
    }
    decodes(body)
}

/// Whether `span`, one string with its quotes, decodes: every escape
/// whole and every surrogate paired.
fn string_decodes(span: &[u8]) -> bool {
    let mut de = serde_json::Deserializer::from_slice(span);
    <Valid as serde::Deserialize>::deserialize(&mut de).is_ok() && de.end().is_ok()
}

/// [`string_decodes`]'s visit: the one string read, nothing kept. Any other value is refused
/// (only a string span is ever handed to it).
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
        f.write_str("a JSON string")
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Valid, E> {
        Ok(Valid)
    }
}

/// The words for a request whose document would not parse where it had to be rewritten for its
/// hop, at 500.
pub const INTERNAL_ERROR: &str = "We received an unexpected internal error. Please try again.";

/// The dialect whose claim `target` names (its path; a query is not the path), where one does.
#[must_use]
pub fn dialect_at(target: &str) -> Option<u32> {
    let path = path_of(target);
    tail::CLAIMS
        .iter()
        .find(|(_, t, _)| *t == path)
        .map(|(_, _, dialect)| *dialect)
}

/// A request target's path: the target up to its query, if it has one.
fn path_of(target: &str) -> &str {
    target.split_once('?').map_or(target, |(path, _)| path)
}

/// The role the one turn of a projected request is shown under: the caller's.
pub const PROJECTED_ROLE: &str = "user";

/// What a hook is shown of one request ([`project_read`]): the entry it routes over, its dialect's
/// name and the characters of the request document, which is the view's one turn and its body.
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
/// that dialect ([`route_read`]; `dialect_of` reads a route's dialect), the dialect's name and the
/// size of the document.
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
/// or a string holding one, or a message with no `role` that is itself an object.
fn rewrite_document(rewrite: &[u8]) -> Option<serde_json::Map<String, serde_json::Value>> {
    use serde_json::Value;
    let Value::Object(mut reply) = parsed(rewrite).ok()? else {
        return None;
    };
    let Value::Array(messages) = reply.remove("messages")? else {
        return None;
    };
    let mut document = None;
    for message in messages {
        let candidate = match message {
            Value::Object(mut fields) => match fields.remove("content") {
                Some(content) => content,
                None if !fields.contains_key("role") => Value::Object(fields),
                None => continue,
            },
            other => other,
        };
        let resolved = match candidate {
            Value::Object(map) => Some(map),
            Value::String(text) => match parsed(text.as_bytes()).ok() {
                Some(Value::Object(map)) => Some(map),
                _ => None,
            },
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
/// provider a request goes to); the plane REFUSES it, and the kernel refuses the unit, never
/// forwarding the body un-rewritten (`busbar_contract::abi::plane::project_faults`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRewritten {
    /// The model the document names.
    pub named: String,
}

/// A rewrite that cannot be applied, and how the plane answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotApplied {
    /// The document names another model: REFUSED, and the kernel refuses the unit.
    Model(ModelRewritten),
    /// The rewrite has a document and the body's whole document does not parse (a member the
    /// head only skipped, carrying a bad escape or a lone surrogate): FAULT, which the kernel always
    /// answers 500 `request rewrite could not be applied`, whatever the hook's `on_error`, the body
    /// never forwarded.
    Fault,
}

/// A rewrite applied in the dialect: the rewrite's document ([`rewrite_document`]) replaces the
/// request wholesale.
pub fn rewritten_read(
    body: &[u8],
    read: &Result<Option<serde_json::Value>, Unrouted>,
    rewrite: &[u8],
    routed: &str,
) -> Result<Option<Vec<u8>>, NotApplied> {
    let Ok(caller_model) = read else {
        return Err(NotApplied::Fault);
    };

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
                // A `model` that is no string is named by its kind only: the refusal is logged,
                // and the value may carry decision content.
                named: match &named {
                    serde_json::Value::String(name) => name.clone(),
                    serde_json::Value::Object(_) => "<an object>".to_string(),
                    serde_json::Value::Array(_) => "<an array>".to_string(),
                    other => other.to_string(),
                },
            }));
        }
    }
    let Some(current) = current else {
        return Ok(None);
    };

    let unchanged = current.len() == document.len() + usize::from(current.contains_key(MODEL))
        && document.iter().all(|(k, v)| current.get(k) == Some(v));
    // The caller's own bytes go on only where no member name repeats, at any depth: `current` is
    // the last value of each name, so an earlier duplicate it never saw would otherwise reach the
    // far end.
    if unchanged && unique_names(body) {
        return Ok(Some(body.to_vec()));
    }
    Ok(Some(replaced(body, &current, &document)))
}

/// Whether no object in the JSON value `bytes` repeats a member name (compared decoded, so an
/// escaped spelling is the same name). Unreadable bytes count as repeating.
fn unique_names(bytes: &[u8]) -> bool {
    use serde::de::{DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
    use std::collections::BTreeSet;

    struct Unique;
    impl<'de> DeserializeSeed<'de> for Unique {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Unique {
        type Value = ();
        fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str("a JSON value")
        }
        fn visit_bool<E>(self, _: bool) -> Result<(), E> {
            Ok(())
        }
        fn visit_i64<E>(self, _: i64) -> Result<(), E> {
            Ok(())
        }
        fn visit_u64<E>(self, _: u64) -> Result<(), E> {
            Ok(())
        }
        fn visit_f64<E>(self, _: f64) -> Result<(), E> {
            Ok(())
        }
        fn visit_str<E>(self, _: &str) -> Result<(), E> {
            Ok(())
        }
        fn visit_unit<E>(self) -> Result<(), E> {
            Ok(())
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            while seq.next_element_seed(Unique)?.is_some() {}
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            let mut seen = BTreeSet::new();
            while let Some(name) = map.next_key::<String>()? {
                if !seen.insert(name) {
                    return Err(serde::de::Error::custom("a member name repeats"));
                }
                map.next_value_seed(Unique)?;
            }
            Ok(())
        }
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    // Only ever called on a body `parsed` accepted, whose depth is already bounded.
    de.disable_recursion_limit();
    Unique.deserialize(&mut de).and_then(|()| de.end()).is_ok()
}

/// [`rewritten_read`]'s request: the caller's members `document` keeps (the `model` always), in the
/// caller's order, each the caller's own bytes where its value is unchanged, then the members the
/// caller did not send.
fn replaced(
    body: &[u8],
    current: &serde_json::Map<String, serde_json::Value>,
    document: &serde_json::Map<String, serde_json::Value>,
) -> Vec<u8> {
    spliced_members(body, current, document).unwrap_or_else(|| {
        let mut whole = document.clone();
        if let Some(model) = current.get(MODEL) {
            whole.insert(MODEL.to_string(), model.clone());
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
    // The root object walked once: each member's span, the last where a name repeats (the one
    // the document holds, as serde's map keeps it).
    let spans = root_spans(body)?;
    let span_of = |key: &str| spans.get(key).copied();
    let mut kept: Vec<(usize, Vec<u8>)> = Vec::with_capacity(current.len());
    for (key, value) in current {
        let span = span_of(key)?;
        let own = body.get(span.start..span.end)?;
        // The caller's bytes for an unchanged value only where no name inside it repeats (the
        // hook read the last of each); otherwise the value as the hook saw it.
        let bytes = if key == MODEL || (document.get(key) == Some(value) && unique_names(own)) {
            own.to_vec()
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

/// Each member of the request's root object with its value's span, the last where a name repeats.
/// The only place [`spliced_members`] walks the body.
fn root_spans(
    body: &[u8],
) -> Option<std::collections::HashMap<String, busbar_contract::spans::Span>> {
    let root = busbar_contract::spans::members(body, 0)?;
    Some(
        root.members
            .iter()
            .map(|m| (m.key.clone(), m.value()))
            .collect(),
    )
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
    /// Its operation's index in [`tail::OP_CLASSES`].
    pub op_class: u32,
    /// Its index in [`tail::DIALECTS`].
    pub dialect: u32,
}

/// Classify an arriving request by its verb and target: its operation, and the dialect its claim
/// is.
#[must_use]
pub fn arrive(verb: &str, target: &str) -> Option<Arrived> {
    let target = path_of(target);
    let row = ops::row_for(verb, target)?;
    let op_class = tail::OP_CLASSES.iter().position(|op| *op == row.op)?;
    let dialect = tail::CLAIMS
        .iter()
        .find(|(v, t, _)| *t == target && v.eq_ignore_ascii_case(verb))?
        .2;
    Some(Arrived {
        op_class: u32::try_from(op_class).ok()?,
        dialect,
    })
}

/// What the plane says to a piece of the caller's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallerAnswer {
    /// Nothing to emit. The kernel keeps the body and re-pushes it on every attempt.
    Keep,
    /// The body ended empty: a decision request carries the caller's state, so there is nothing
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
    /// The dialect fields. The kernel adds the auth fields for the outbound style the plane's
    /// one need names ([`tail::NEEDS`], [`tail::OUTBOUND_STYLE`]).
    pub fields: [(&'static str, &'static [u8]); 1],
}

/// Answer an ATTEMPT piece of a unit of `dialect`: the same request on every attempt, whichever
/// member the kernel picked, at the verb and path of the dialect's claim; `None` for a dialect no
/// claim names.
#[must_use]
pub fn attempt_request(dialect: u32) -> Option<FarEndRequest> {
    let (verb, target, _) = tail::CLAIMS
        .iter()
        .copied()
        .find(|(_, _, d)| *d == dialect)?;
    Some(FarEndRequest {
        verb,
        target,
        fields: [(FIELD_CONTENT_TYPE, CONTENT_TYPE_JSON)],
    })
}

/// Where a count the plane reports comes from: the far end's own counts are kept apart from the
/// plane's floor; both bill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The far end's answer states it (`UNITS_REPORTED`); the request fee is reported so too, as
    /// the contract reports a fee unit incurred (0 or 1 on its class).
    Reported,
    /// The far end reports a fractional count; `amount` carries exact micro-units.
    ReportedMicros,
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
    /// The cumulative count: whole units except for [`Source::ReportedMicros`].
    pub amount: u64,
}

/// A usage member the far end's success answer carries but that is not an exact representable
/// count (or a required count it does not carry): located, never a silent 0.
/// Present unreadable counts are refused; readable siblings still bill.
/// Names the member's pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnreadableUsage(pub &'static str);

/// What one answer bills: the decision count, the input and output tokens, and the request fee,
/// each [`Units`] naming its own [`tail::BILLABLE_CLASSES`] class. `None` = nothing in that class.
pub type Counts = [Option<Units>; 4];

/// A usage member as the answer states it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Member {
    /// The answer has no such member.
    #[default]
    Absent,
    /// An exact non-negative count within the declared store range.
    Exact(Count),
    /// Present, and not a representable count.
    Unreadable,
}

/// What a member of the answer is to the reading: `/error`, the dialect's usage parent (`/usage`
/// for `jev`, `/execution` for `nanojev`), one of the three usage
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
struct Usage {
    parent: &'static [u8],
    counted: &'static [(&'static [u8], &'static str)],
    /// The decision count is required: a success with none bills the plane's floor and raises
    /// 5029 ([`UnreadableUsage`]).
    required: bool,
}

/// Each dialect's [`Usage`], in [`tail::DIALECTS`]' order: `jev` reads `/usage/units` (one decision
/// when absent) and its tokens; `nanojev` reads the upstream's own billing unit,
/// `/execution/states`, and a success with no count bills the plane's floor and raises 5029.
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

// Every dialect has its usage row: a dialect added without one fails to build.
const _: () = assert!(USAGE.len() == tail::DIALECTS.len());

/// The longest member name compared ("output_tokens"), with room.
const KEY_CAP: usize = 16;
/// What a member name's escape that names no ASCII character decodes to in the name read: a byte
/// no UTF-8 text holds, so no name compared matches it.
const NOT_ASCII: u8 = 0xFF;
/// Bounded decimal spelling, including fractions and exponent notation: a count spelled in fewer
/// bytes is read; one that fills the buffer (32 bytes or more, kept truncated) is unreadable.
const SCALAR_CAP: usize = 32;

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
    /// The member name being read, its escapes decoded (cut at [`KEY_CAP`]: a longer one is none
    /// compared).
    key: [u8; KEY_CAP],
    key_len: usize,
    /// In a member name: the hex digits of a `\u` escape still to read, and their value so far.
    hex_left: u8,
    hex: u32,
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
    /// The answer's first significant byte was read, and it opened an object: an answer whose
    /// top-level value is no object states no usage a reading can find.
    rooted: bool,
    root_object: bool,
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

    /// Read one far-end piece (relayed to the caller as it is, by the door).
    pub fn piece(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.byte(b);
        }
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
            (Slot::Usage, _) => self.members = [Member::Absent; 3],
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
                self.members[i] = if raw == b"null" {
                    Member::Absent
                } else if self.scalar_len > SCALAR_CAP - 1 {
                    Member::Unreadable
                } else {
                    count(raw).map_or(Member::Unreadable, Member::Exact)
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
        // A repeated usage parent, or a repeated count inside it, supersedes what was read before
        // it from the key on: an answer cut before the new value is read whole bills none of it.
        match (self.depth, named) {
            (1, Slot::Usage) => self.members = [Member::Absent; 3],
            (2, Slot::Count(i)) if self.object[2] && self.at[1] == Slot::Usage => {
                self.members[i] = Member::Absent;
            }
            _ => {}
        }
        if let 1 | 2 = self.depth {
            self.at[self.depth as usize] = named;
        }
    }

    /// One decoded byte of the member name being read.
    fn key_byte(&mut self, b: u8) {
        if self.key_len < KEY_CAP {
            self.key[self.key_len] = b;
        }
        self.key_len = self.key_len.saturating_add(1);
    }

    /// One byte of a member name, read as JSON reads it (RFC 8259 section 7): an escape decoded,
    /// `\u0075` and `u` being one name.
    fn key_char(&mut self, b: u8, escaped: bool) {
        if escaped {
            match b {
                b'u' => (self.hex_left, self.hex) = (4, 0),
                b'b' => self.key_byte(0x08),
                b'f' => self.key_byte(0x0c),
                b'n' => self.key_byte(b'\n'),
                b'r' => self.key_byte(b'\r'),
                b't' => self.key_byte(b'\t'),
                other => self.key_byte(other),
            }
        } else if self.hex_left > 0 {
            let digit = char::from(b).to_digit(16).unwrap_or(0x1_0000);
            self.hex = self.hex.saturating_mul(16).saturating_add(digit);
            self.hex_left -= 1;
            if self.hex_left == 0 {
                let decoded = u8::try_from(self.hex).ok().filter(u8::is_ascii);
                self.key_byte(decoded.unwrap_or(NOT_ASCII));
            }
        } else {
            self.key_byte(b);
        }
    }

    /// One byte of the answer.
    fn byte(&mut self, b: u8) {
        if self.rooted && self.root_object && self.depth == 0 {
            return;
        }
        if self.in_str {
            if self.escaped {
                self.escaped = false;
                if self.str_key {
                    self.key_char(b, true);
                }
                return;
            }
            if self.str_key && self.hex_left > 0 && matches!(b, b'"' | b'\\') {
                self.hex_left = 0;
                self.key_byte(NOT_ASCII);
            }
            if b == b'"' {
                self.in_str = false;
                if self.str_key {
                    self.end_key();
                }
            } else if b == b'\\' {
                self.escaped = true;
            } else if self.str_key {
                self.key_char(b, false);
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

        if !self.rooted && !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0xEF | 0xBB | 0xBF) {
            self.rooted = true;
            self.root_object = b == b'{';
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
    /// `/error` member (a `null` one is none).
    #[must_use]
    pub fn success(&self, status: Option<u32>) -> bool {
        status.is_some_and(|s| (200..300).contains(&s)) && self.settled().0 != Some(true)
    }

    /// Whether a completed provider measurement was explicitly refused.
    #[must_use]
    pub fn refused_count(&self) -> bool {
        self.settled()
            .1
            .iter()
            .any(|member| matches!(member, Member::Unreadable))
    }

    /// What the answer bills, read at the last piece: a non-success bills nothing; a success bills one `decision` (the plane's floor,
    /// [`Source::Floor`]), or the whole count its
    /// `/usage/units` reports (the hosted provider's count), its `/usage/input_tokens`
    /// and `/usage/output_tokens` where it reports them, and the request fee (`1` of
    /// [`tail::FEE_PER_REQUEST`]: a refused or failed unit owes none).
    pub fn counts(&self, status: Option<u32>) -> Result<Counts, UnreadableUsage> {
        if !self.success(status) {
            return Ok([None; 4]);
        }
        let (_, members) = self.settled();
        let usage = self.usage();
        if !self.root_object {
            return Err(UnreadableUsage(usage.counted[0].1));
        }
        let mut whole = [None; 3];
        for (i, (_, pointer)) in usage.counted.iter().enumerate() {
            whole[i] = match members[i] {
                Member::Absent if i == 0 && usage.required => return Err(UnreadableUsage(pointer)),
                Member::Absent => None,
                Member::Exact(n) => Some(n),
                Member::Unreadable => return Err(UnreadableUsage(pointer)),
            };
        }
        Ok(Self::billed(whole))
    }

    /// Readable sibling counts after [`Self::counts`] reports [`UnreadableUsage`].
    /// A present unreadable decision count is refused, never replaced by a floor.
    /// Only an absent count retains the plane's one-decision floor.
    #[must_use]
    pub fn floored(&self, status: Option<u32>) -> Counts {
        if !self.success(status) {
            return [None; 4];
        }
        let (_, members) = self.settled();
        let mut whole = [None; 3];
        for (i, _) in self.usage().counted.iter().enumerate() {
            whole[i] = match members[i] {
                Member::Exact(n) => Some(n),
                Member::Absent | Member::Unreadable => None,
            };
        }
        let mut billed = Self::billed(whole);
        if matches!(members[0], Member::Unreadable) {
            billed[0] = None;
        }
        billed
    }

    /// What an answer the far end did not finish bills (its last piece flagged `PIECE_CUT`: its
    /// connection failed, or the send's deadline passed, after its head): only each count the far
    /// end reported whole before the cut and did not supersede by repeating its member or its
    /// usage parent, and nothing for a non-success.
    #[must_use]
    pub fn cut(&self, status: Option<u32>) -> Counts {
        if !self.success(status) {
            return [None; 4];
        }

        let reported = |class: u32, member: Member| match member {
            Member::Exact(amount) => Some(reported(class, amount)),
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
    fn billed(whole: [Option<Count>; 3]) -> Counts {
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
            Some(reported(tail::CLASS_FEE_INDEX, Count::ONE)),
        ]
    }
}

/// Read a reported decimal exactly, refusing negative, inexact, or out-of-store-range counts.
#[must_use]
pub fn count(raw: &[u8]) -> Option<Count> {
    Count::parse_bytes(raw)
        .ok()
        .filter(|n| !n.is_negative() && n.fits_a_64_bit_column())
}

/// Preserve the whole-unit ABI for integral quantities; fractional ones declare micro-units.
fn reported(class: u32, count: Count) -> Units {
    let micros = u64::try_from(count.micros()).expect("validated non-negative store count");
    let (source, amount) = if micros % 1_000_000 == 0 {
        (Source::Reported, micros / 1_000_000)
    } else {
        (Source::ReportedMicros, micros)
    };
    Units {
        class,
        source,
        amount,
    }
}

/// THE CALLER FIELDS THE FAR END NEVER RECEIVES, besides the credential carriers: the ones the
/// plane writes itself (the document type) and the ones naming the hop (`host`).
pub const NOT_RELAYED: &[&str] = &[FIELD_CONTENT_TYPE, "host", "accept-encoding"];

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
/// carriers' (a same-dialect relay passes every header but the governed set).
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
#[must_use]
pub fn refusal_body(status: u16, text: &str) -> Vec<u8> {
    let code = match status {
        403 => "unsupported_operation",
        404 => "invalid_params",
        429 => "rate_limit",
        400..=499 => "invalid_request",
        500..=599 => "internal",
        _ => "unsupported_operation",
    };
    error_body(code, text)
}

#[cfg(test)]
#[path = "tests/driven.rs"]
mod tests;
