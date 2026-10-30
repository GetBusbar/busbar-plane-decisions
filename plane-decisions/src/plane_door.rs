// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! The decisions plane's door: the plane kind's memory-ABI table (`busbar_contract::abi::plane`)
//! built with the SDK's `plugin_door!` on its safe surface, over this plane's answers to the plane
//! driver ([`crate::driven`]). [`door`] is the linked door; the same function is the dropped-in
//! door once a `cdylib` exports it (`busbar-plane-decisions-plugin`, through
//! `busbar_contract::export_door!`), so the two cannot answer differently.
//!
//! * The Statement: the plane's key and version, `decisions:` declared and `providers:` consumed,
//!   its one outbound need, the tail [`TAIL`] (every list read off [`crate::driven::tail`]), and
//!   its extensions [`EXTENSIONS`]: the plugin's whole `declares` document.
//! * `validate`, `open`, `refresh`: the settings blob read as the `decisions:` section
//!   ([`crate::config::DecisionsSection`]), each model's dialect resolved ([`routes`]: its own
//!   `protocol`, else its provider's as the host stated it at `open`); a generation's snapshot
//!   claims [`crate::driven::served_claims`] for its models' dialects, lists every model the
//!   section configures (the kernel filters them by scope and `/v1/models` appends them), and
//!   binds no audience (the operation is served on the plain data plane to a keyed caller).
//!   `retire` drops a generation.
//! * `arrive`: [`crate::driven::arrive`], routed over the model the request names
//!   ([`crate::driven::route_model`]); an unclaimed request is refused at 404, and one that routes
//!   over no model at its [`crate::driven::Unrouted`] status.
//! * `on_piece`: the ATTEMPT's request head ([`crate::driven::attempt_request`]), the caller's body to the
//!   far end unchanged ([`crate::driven::caller_piece`]) but for the routed model's `upstream_model`
//!   ([`crate::driven::spliced`]), and the far end's answer relayed unchanged with its count read at
//!   the last piece ([`crate::driven::FarEndReading`]).
//! * `refusal`: [`crate::driven::refusal_body`]; the refused unit's state is dropped.
//! * `cancel`: the unit the host names (`PlaneCancelIn::unit`), else the one on the cancelled
//!   ticket, is dropped, `CANCEL_ABORTED` before its far end answered and `CANCEL_OK_PARTIAL` after.
//! * `project`: the hook view of a unit's request ([`crate::driven::project_hook`]): the entry it routes
//!   over as the view's pool, its dialect, the request document as the one turn a content-granted
//!   hook reads, and a request-stage hook's rewrite applied in the dialect
//!   ([`crate::driven::rewritten`]) and answered as the body the kernel keeps.
//! * `serve` is REFUSED: the plane publishes no admin route. `hydrate`, `start`, `tick`, `drive`,
//!   `release` and `close` hold nothing.
//!
//! A unit's state lives from its arrival to its end (its last far-end piece, its refusal or its
//! cancel), and at most [`MAX_UNITS`] live at once: an arrival past that is refused, and a live
//! unit is never dropped to make room.
//!
//! Each generation's routes are kept beside its snapshot, and an arrival routes over the newest
//! generation the host still holds: a generation the host refuses after this plane's `refresh` is
//! retired by the host, and its routes go with it, so the running generation keeps serving.

use std::collections::BTreeMap;

use busbar_contract::abi::host::conn::connector::{
    Need, DIRECTION_OUTBOUND, EGRESS_PROVIDER, KEEP_NAMED,
};
use busbar_contract::abi::mechanism::call::{AbiStr, Blob, OutHead, Outcome, Span};
use busbar_contract::abi::mechanism::door::{
    Section, Statement, SECTION_CONSUMED, SECTION_DECLARING,
};
use busbar_contract::abi::mechanism::lifecycle::GenIn;
use busbar_contract::abi::mechanism::ticket::Ticket;
use busbar_contract::abi::plane::{
    ArriveIn, ArriveOut, BillableClass, DialectAuth, OnPieceIn, OnPieceOut, OpClass, OutField,
    PlaneOpenIn, PlaneSnapshot, PlaneTail, ProjectIn, ProjectOut, RefusalIn, RefusalOut, ServeIn,
    ServeOut, UnitCount, ANSWER_LAYOUT, CLAIM_EXACT, EMIT_DONE, EMIT_TO_FAR_END, FROM_CALLER,
    FROM_FAR_END, FROM_KERNEL, INGRESS_REQUEST_RESPONSE, PIECE_CUT, PIECE_FIELDS, PIECE_HAS_STATUS,
    PIECE_LAST, PRINCIPAL_REQUIRED, ROUTE_DIRECT, SHAPE_WHOLE, SPAN_ABSENT, UNITS_ESTIMATED,
    UNITS_FLOOR, UNITS_REPORTED,
};
use busbar_contract::abi::plane::{PlaneCancelIn, PlaneCancelOut};
use busbar_contract::abi::sdk::door::{abi_str, statement};
use busbar_contract::abi::sdk::life::Refusal;
use busbar_contract::abi::sdk::plane::{cancel_disposition, Ending, UnitCancel};
use busbar_contract::abi::sdk::plane_life::{Opened, PlaneLife, Published};
use busbar_contract::abi::sdk::publish::{ClaimSpec, SnapshotSpec};
use busbar_contract::abi::sdk::{Generations, HostBuf, Instance, Keyed, Lent, Out, Safe, SafeSlot};

use crate::claims;
use crate::codec::{CONTENT_TYPE_JSON, FIELD_CONTENT_TYPE, FIELD_RETRY_AFTER};
use crate::config::DecisionsSection;
use crate::driven::{self, tail, CallerAnswer, FarEndReading, Source};
use crate::meta::PLANE_KEY;

/// The version the Statement names: the crate's (a test pins the two equal).
pub const VERSION: &str = "1.6.0";

/// The most calls the kernel keeps in flight on one instance.
const MAX_INFLIGHT: u32 = 64;

/// The most units the instance keeps state for at once; past it, a new arrival is refused
/// ([`BUSY`]) and no live unit is dropped.
pub const MAX_UNITS: usize = 4096;

/// [`ArriveOut::refusal`]: the request names no operation this plane claims.
pub const UNCLAIMED: u32 = 1;

/// The status an unclaimed request is refused at.
const STATUS_NOT_FOUND: u32 = 404;

/// [`ArriveOut::refusal`]: the request routes over no configured model ([`driven::Unrouted`]).
pub const UNROUTED: u32 = 2;

/// [`ArriveOut::refusal`]: the instance holds [`MAX_UNITS`] live units; the arrival is refused at
/// [`STATUS_BUSY`], about the entry it routed over, before anything is charged.
pub const BUSY: u32 = 3;

/// The status a [`BUSY`] arrival is refused at: an arrive refusal is the caller's to retry (4xx).
const STATUS_BUSY: u32 = 429;

/// The words a [`BUSY`] arrival is refused with: the sibling planes' sentence for a full unit table.
const BUSY_WORDS: &str =
    "busbar is holding as many decision calls as it admits at once; retry once one has ended";

/// [`ArriveOut::refusal`]: the request names a model other than its route's `upstream_model` and
/// its document does not parse, so the far end cannot be sent the routed model
/// ([`driven::spliceable`]); refused at [`STATUS_INTERNAL`], about the entry it routed over.
pub const UNSPLICEABLE: u32 = 4;

/// The status an [`UNSPLICEABLE`] arrival is refused at.
const STATUS_INTERNAL: u32 = 500;

/// The human label.
const LABEL: &str = "Decisions";

/// `decisions:` declared, `providers:` consumed ([`tail::SECTION_DECLARING`],
/// [`tail::SECTIONS_CONSUMED`]).
const SECTIONS: &[Section] = &[
    Section {
        name: abi_str(tail::SECTION_DECLARING),
        flags: SECTION_DECLARING,
        ..Section::ZERO
    },
    Section {
        name: abi_str(tail::SECTIONS_CONSUMED[0]),
        flags: SECTION_CONSUMED,
        ..Section::ZERO
    },
];

const DIALECTS: &[AbiStr] = &[abi_str(tail::DIALECTS[0]), abi_str(tail::DIALECTS[1])];

/// Each dialect's default outbound style ([`tail::DIALECT_AUTH`]).
const DIALECT_AUTH: &[DialectAuth] = &[dialect_auth(0), dialect_auth(1)];

/// [`tail::DIALECT_AUTH`]'s row `i`, on the ABI.
const fn dialect_auth(i: usize) -> DialectAuth {
    DialectAuth {
        dialect: tail::DIALECT_AUTH[i].0,
        // No provider's `protocol` names a decisions dialect for a lane, and no caller signs one.
        style: abi_str(tail::DIALECT_AUTH[i].1),
        params: Blob::ABSENT,
        ..DialectAuth::ZERO
    }
}

const SCOPE_KINDS: &[AbiStr] = &[abi_str(tail::SCOPE_KINDS[0])];

const OP_CLASSES: &[OpClass] = &[op_class(0), op_class(1)];

/// [`tail::OP_CLASSES`]' class `i`, on the ABI.
const fn op_class(i: usize) -> OpClass {
    OpClass {
        op: abi_str(tail::OP_CLASSES[i].as_str()),
        name: abi_str(tail::OP_CLASSES[i].as_str()),
    }
}

const BILLABLE_CLASSES: &[BillableClass] = &[billable(0), billable(1), billable(2), billable(3)];

/// [`tail::BILLABLE_CLASSES`]' class `i`, on the ABI.
const fn billable(i: usize) -> BillableClass {
    BillableClass {
        class: abi_str(tail::BILLABLE_CLASSES[i].0.as_str()),
        family: abi_str(tail::BILLABLE_CLASSES[i].1),
    }
}

/// The fee unit the plane counts ([`tail::FEE_UNITS`]).
const FEE_UNITS: &[AbiStr] = &[abi_str(tail::FEE_UNITS[0])];

/// The far end's one response field the plane relays: the document type of the answer.
const KEEP_RESPONSE_HEADERS: &[AbiStr] = &[abi_str(FIELD_CONTENT_TYPE)];

/// The plane's one need ([`tail::NEEDS`]): outbound to a configured provider, over the claim's
/// transport, decorated by the kernel in the outbound style it names ([`tail::OUTBOUND_STYLE`],
/// the operator's credential as a bearer, which the plane never holds).
pub const NEEDS: &[Need] = &[{
    let mut n = Need::EMPTY.with_keep_response_headers(KEEP_RESPONSE_HEADERS);
    n.direction = DIRECTION_OUTBOUND;
    n.egress_class = EGRESS_PROVIDER;
    n.transport = abi_str(tail::NEEDS[0].0);
    n.auth = abi_str(tail::NEEDS[0].1);
    n.keep_mode = KEEP_NAMED;
    n
}];

/// The Statement tail: the plane's static facts, every list [`crate::driven::tail`]'s.
pub const TAIL: &PlaneTail = &{
    let mut t = PlaneTail::EMPTY
        .with_dialects(DIALECTS)
        .with_dialect_auth(DIALECT_AUTH)
        .with_scope_kinds(SCOPE_KINDS)
        .with_op_classes(OP_CLASSES)
        .with_billable_classes(BILLABLE_CLASSES)
        .with_fee_units(FEE_UNITS);
    t.ingress = INGRESS_REQUEST_RESPONSE;
    // The kernel gathers the caller's whole body and pushes it as one piece.
    t.dispatch_shape = SHAPE_WHOLE;
    t.answer_layout = ANSWER_LAYOUT;
    t.scope = abi_str(tail::SCOPE_KINDS[0]);
    t.label = abi_str(LABEL);
    t.subject_noun = abi_str(tail::SUBJECT_NOUN);
    t.admin_noun = abi_str(tail::ADMIN_NOUN);
    t.audit_kind = abi_str(tail::AUDIT_KIND);
    t
};

busbar_contract::statement_extensions!(
    /// The Statement's extensions: this plugin's whole `declares` document.
    EXTENSIONS: [(
        busbar_contract::abi::mechanism::extensions::EXT_DECLARES,
        include_bytes!("../declares.json")
    )]
);

/// The Statement: the plane's key and version, its sections, its need, its tail and its
/// extensions.
pub const STATEMENT: Statement = Statement {
    extensions: EXTENSIONS,
    ..statement(PLANE_KEY, VERSION, MAX_INFLIGHT)
        .with_sections(SECTIONS)
        .with_needs(NEEDS)
        .with_kind_tail(TAIL)
};

/// The settings blob read as the `decisions:` section; an empty blob is the empty section.
///
/// The blob is one JSON object keyed by section: `{decisions: <section>}` beside each section the
/// plane consumes that the deployment writes (`providers:`). The plane reads its own key; a
/// consumed section's facts reach it at open (`PlaneOpenIn::providers`). Only the plane's own verb and the sections it consumes may sit in
/// the keyed object: any other member beside the verb makes it no wrapper. The bare section is read
/// too, since a section never carries a member named after its own verb (its grammar refuses
/// unknown members), so the two cannot be confused.
///
/// # Errors
///
/// The section's first broken rule, in the grammar's words.
pub fn read_settings(settings: &[u8]) -> Result<DecisionsSection, String> {
    if settings.is_empty() {
        return Ok(DecisionsSection::default());
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(settings).map_err(|e| e.to_string())?;
    let section = match value.as_object_mut() {
        Some(blob)
            if blob.contains_key(tail::SECTION_DECLARING)
                && blob.keys().all(|k| {
                    k == tail::SECTION_DECLARING || tail::SECTIONS_CONSUMED.contains(&k.as_str())
                }) =>
        {
            blob.remove(tail::SECTION_DECLARING)
                .unwrap_or(serde_json::Value::Null)
        }
        _ => value,
    };
    if section.is_null() {
        return Ok(DecisionsSection::default());
    }
    serde_json::from_value(section).map_err(|e| e.to_string())
}

/// `validate`'s blob (`{<verb>: <section>}`), read at the `decisions:` section. An empty blob, or one that writes no
/// `decisions:`, is the empty section.
///
/// # Errors
///
/// A blob that is not a map of sections, or the section's first broken rule.
pub fn read_dealt(settings: &[u8]) -> Result<DecisionsSection, String> {
    match busbar_contract::abi::sdk::plane::dealt_section(settings, tail::SECTION_DECLARING)? {
        None => Ok(DecisionsSection::default()),
        Some(section) => serde_json::from_value(section).map_err(|e| e.to_string()),
    }
}

/// One generation's snapshot for a section whose models are of `dialects`: the claims
/// [`driven::served_claims`] serves, each an exact target over the claim's transport, wearing its
/// own dialect for a refusal before `arrive`. It binds no audience: the operation is served on the
/// plain data plane to a keyed caller, as the llm plane's is.
#[must_use]
pub fn snapshot_spec(dialects: &[u32]) -> SnapshotSpec {
    SnapshotSpec {
        claims: driven::served_claims(dialects)
            .into_iter()
            .map(|(verb, target, dialect)| ClaimSpec {
                refusal_dialect: u16::try_from(dialect).unwrap_or(0),
                ..ClaimSpec::new(verb, target, claims::TRANSPORT, CLAIM_EXACT)
            })
            .collect(),
        ..SnapshotSpec::default()
    }
}

/// What a claimed arrival routes over: a model's dialect, and its `upstream_model`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRoute {
    /// Its index in [`tail::DIALECTS`].
    pub dialect: u32,
    /// Its `upstream_model`.
    pub upstream: Option<String>,
}

/// Every model's route under `section`: its dialect, its own `protocol` override or else its
/// provider's protocol (`protocols`, the provider facts the host handed at `open`).
///
/// # Errors
///
/// A model whose dialect is none this plane speaks, named with the dialects it does; and a model with no override whose provider the host stated no
/// protocol for, named with that provider: it has no dialect, and none is guessed for it.
pub fn routes(
    section: &DecisionsSection,
    protocols: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, ModelRoute>, String> {
    section
        .models
        .iter()
        .map(|(name, model)| {
            let protocol = model
                .protocol
                .as_deref()
                .or_else(|| protocols.get(&model.provider).map(String::as_str))
                .ok_or_else(|| {
                    format!(
                        "decisions.models.{name} names provider '{}', whose protocol the host did \
                         not state when the plane opened, and states no `protocol` of its own; the \
                         decisions plane speaks only: {}.",
                        model.provider,
                        tail::DIALECTS.join(", ")
                    )
                })?;
            Ok((
                name.clone(),
                ModelRoute {
                    dialect: dialect_named(name, protocol)?,
                    upstream: model.upstream_model.clone(),
                },
            ))
        })
        .collect()
}

/// Every model's own `protocol` override under `section` is a dialect this plane speaks
/// (`validate`, handed no provider facts: a provider's protocol is judged by the kernel against
/// the dialects the Statement states).
///
/// # Errors
///
/// As [`routes`]' for an unspoken dialect.
pub fn spoken_overrides(section: &DecisionsSection) -> Result<(), String> {
    section
        .models
        .iter()
        .filter_map(|(name, model)| model.protocol.as_deref().map(|p| (name, p)))
        .try_for_each(|(name, protocol)| dialect_named(name, protocol).map(drop))
}

/// The index of `protocol`, model `name`'s resolved one, in [`tail::DIALECTS`], or the refusal
/// naming the dialects the plane speaks.
fn dialect_named(name: &str, protocol: &str) -> Result<u32, String> {
    driven::dialect_of(protocol).ok_or_else(|| {
        format!(
            "decisions.models.{name} resolves to protocol '{protocol}', but the decisions plane \
             speaks only: {}.",
            tail::DIALECTS.join(", ")
        )
    })
}

/// One generation's snapshot for `section`'s `routes`: [`snapshot_spec`]'s claims for its models'
/// dialects, and every model it configures listed (the kernel scope-filters them; `/v1/models`
/// appends them), in ascending name order.
#[must_use]
pub fn generation_spec(routes: &BTreeMap<String, ModelRoute>) -> SnapshotSpec {
    let dialects: Vec<u32> = routes.values().map(|r| r.dialect).collect();
    SnapshotSpec {
        listed: routes.keys().cloned().collect(),
        ..snapshot_spec(&dialects)
    }
}

/// One unit the instance keeps, from its ATTEMPT to the far end's last piece.
#[derive(Debug, Default)]
struct Unit {
    /// An ATTEMPT opened the request bound for the far end; the caller's body follows it there.
    attempt: bool,
    /// The far end's answer, read as it is relayed.
    reading: FarEndReading,
    /// The piece answered short: its re-call carries the same piece, which is not read twice.
    recall: bool,
    /// What a `more = 1` answer still owes the host, and how much of it is written.
    owed: Vec<u8>,
    /// How much of [`Unit::owed`] is written.
    at: usize,
    /// The `EMIT_*` bits the owed bytes carry.
    owed_flags: u32,
    /// The caller's fields the far end receives ([`driven::relayed_fields`]), kept from `arrive`
    /// (the caller's head crosses once) for every ATTEMPT of the unit.
    caller: driven::RelayedFields,
    /// The ticket the unit's last piece crossed on: what a `cancel` names it by.
    ticket: Option<Ticket>,
    /// The far end's status, read off its first piece; the request fee is judged on it.
    status: Option<u32>,
    /// The routed model's `upstream_model`, kept from `arrive`: the caller's top-level `model` is
    /// spliced to it on the way to the far end where the two differ ([`driven::spliced`]).
    upstream: Option<String>,
    /// The routes of the generation `arrive` routed the unit over: `project` sees the unit as it
    /// was routed, whatever generation the host holds by then.
    routes: Option<std::sync::Arc<Routes>>,
}

/// Every model one generation routes over, directly, by name, each with its dialect and
/// `upstream_model`.
pub type Routes = BTreeMap<String, ModelRoute>;

/// One instance: every live generation's snapshot with the routes it was built from, and the units
/// in flight.
#[derive(Debug)]
pub struct DecisionsDoor {
    /// Each live generation's snapshot, and beside it the routes a claimed arrival is routed over
    /// while it is the newest the host holds ([`Generations::current`]).
    generations: Generations<PlaneSnapshot, Routes>,
    units: Keyed<u64, Unit>,
    /// Each provider's protocol, as the host stated it at `open` (`PlaneOpenIn::providers`).
    protocols: BTreeMap<String, String>,
}

impl DecisionsDoor {
    /// The routes of the newest generation the host holds: a refused generation, retired by the
    /// host, routes nothing, and the one it kept serving routes again.
    fn routes(&self) -> std::sync::Arc<Routes> {
        self.generations.current().unwrap_or_default()
    }
}

/// One slot body on the SDK's safe surface, over this plane's [`DecisionsDoor`].
macro_rules! slot {
    ($(#[$doc:meta])* $name:ident, $in:ty, $out:ty,
     |$inst:pat_param, $input:pat_param, $o:pat_param| $body:block) => {
        $(#[$doc])*
        #[derive(Debug)]
        pub struct $name;
        impl SafeSlot for $name {
            type In = $in;
            type Out = $out;
            type State = DecisionsDoor;
            fn call($inst: Instance<'_, DecisionsDoor>, $input: Lent<'_, $in>, $o: Out<'_, $out>)
                -> Outcome $body
        }
    };
}

/// The decisions plane's lifecycle over the SDK's
/// ([`plane_life`](busbar_contract::abi::sdk::plane_life)): `validate`, `open` and
/// `refresh` are the plane's reads; `retire` drops a generation's snapshot and its routes (a
/// generation the host refused is retired too, so the one it kept serving routes arrivals again);
/// `tick`, `drive`, `release` and `close` are the SDK's defaults (no tick wanted, no unsolicited
/// output, READY under no lease); `cancel` is the plane's own.
impl PlaneLife for DecisionsDoor {
    /// Each generation's routes, kept beside its snapshot.
    type Held = Routes;

    fn generations(&self) -> &Generations<PlaneSnapshot, Routes> {
        &self.generations
    }

    /// `validate`: the settings read as the section, or refused in the grammar's words.
    fn validate(settings: &[u8]) -> Result<(), Refusal> {
        // A model's own `protocol` must be a dialect this plane speaks; its provider's is judged
        // by the kernel against the dialects the Statement states.
        read_dealt(settings)
            .and_then(|section| spoken_overrides(&section))
            .map_err(Refusal::refused)
    }

    /// `open`: the instance, and the first generation's snapshot. REFUSED in the grammar's words,
    /// as `validate` and `refresh` refuse the same settings.
    fn open(input: &Lent<'_, PlaneOpenIn>) -> Result<Opened<Self>, Refusal> {
        let section = read_settings(input.open().settings().bytes()).map_err(Refusal::refused)?;
        let protocols: BTreeMap<String, String> = input
            .providers()
            .iter()
            .filter_map(|p| {
                Some((
                    p.name().as_str().ok()?.to_string(),
                    p.protocol().as_str().ok()?.to_string(),
                ))
            })
            .collect();
        let routed = routes(&section, &protocols).map_err(Refusal::refused)?;
        Ok(Opened {
            plane: DecisionsDoor {
                generations: Generations::new(),
                units: Keyed::new(),
                protocols,
            },
            first: Published {
                spec: generation_spec(&routed),
                held: routed,
            },
        })
    }

    /// `refresh`: the new section judged, and the next generation's snapshot. The routes ride
    /// beside the snapshot: they route arrivals once the host holds this generation as its
    /// newest, and go with it when the host retires it (a reload the host refuses after this
    /// refresh).
    fn refresh(&self, settings: &[u8], _: u64) -> Result<Published<Routes>, Refusal> {
        let section = read_settings(settings).map_err(Refusal::refused)?;
        let routed = routes(&section, &self.protocols).map_err(Refusal::refused)?;
        Ok(Published {
            spec: generation_spec(&routed),
            held: routed,
        })
    }

    /// `cancel`: the unit the host names is dropped (`PlaneCancelIn::unit`, the key its `arrive`
    /// carried, so a unit no piece reached is found), else the one on the cancelled ticket (a
    /// cancel of an op in flight). Before its far end answered it was aborted (nothing billed);
    /// after, it was cut partway.
    fn cancel(
        instance: Instance<'_, Self>,
        input: Lent<'_, PlaneCancelIn>,
        mut out: Out<'_, PlaneCancelOut>,
    ) -> Outcome {
        let cancel = UnitCancel::of(&input);
        let answered = instance.get().is_some_and(|plane| {
            cancel.end(
                &plane.units,
                Ending::Remove,
                |unit| unit.ticket,
                |unit| unit.status.is_some(),
            )
        });
        out.set(|o| &o.cancel.disposition, cancel_disposition(answered));
        Outcome::Ready
    }
}

slot!(
    /// `arrive`: a claimed request's op class, principal need, dialect and route (the model it
    /// names, or the only one configured, directly); an unclaimed one is
    /// refused at 404, and one that routes over no model at its [`driven::Unrouted`] status.
    Arrive, ArriveIn, ArriveOut, |instance, input, mut out| {
        let verb = input.method().as_str().unwrap_or_default();
        let target = input.target().as_str().unwrap_or_default();
        let Some(arrived) = driven::arrive(verb, target) else {
            out.set(|o| &o.refusal, UNCLAIMED);
            out.set(|o| &o.refusal_status, STATUS_NOT_FOUND);
            return out.fail(Refusal::bare());
        };
        out.set(|o| &o.op_class, arrived.op_class);
        out.set(|o| &o.dialect, arrived.dialect);
        // A decision is billed to someone: the provider account is the deployment's, and the unit
        // is the caller's.
        out.set(|o| &o.principal_need, PRINCIPAL_REQUIRED);
        if let Some(plane) = instance.get() {
            let body = input.body().bytes();
            let generation = plane.routes();
            let routes: &Routes = &generation;
            let routed = driven::route_named(body, routes, |r| r.dialect == arrived.dialect);
            let (upstream, named) = match routed {
                Ok((model, routed, requested)) => {
                    out.route(ROUTE_DIRECT, model);
                    (routed.upstream.clone(), requested)
                }
                Err(unrouted) => {
                    out.set(|o| &o.refusal, UNROUTED);
                    out.set(|o| &o.refusal_status, u32::from(unrouted.status()));
                    return out.fail(Refusal::refused(unrouted.text()));
                }
            };
            // A request whose model the far end could not be sent as the routed one is refused
            // here, never relayed with the caller's model.
            if let Some(up) = upstream.as_deref() {
                if !driven::spliceable_named(body, named.as_deref(), up) {
                    out.set(|o| &o.refusal, UNSPLICEABLE);
                    out.set(|o| &o.refusal_status, STATUS_INTERNAL);
                    return out.fail(Refusal::refused(driven::INTERNAL_ERROR));
                }
            }
            // The caller's head crosses here once: keep what the far end receives of it.
            let caller = driven::relayed_fields(input.fields().iter().map(|f| {
                (
                    f.name().bytes(),
                    f.value().bytes(),
                )
            }));
            let unit = input.unit();
            let kept = plane.units.with_all(|units| {
                let Some(kept) = unit_of(units, unit) else {
                    return false;
                };
                kept.caller = caller;
                kept.upstream = upstream;
                kept.routes = Some(std::sync::Arc::clone(&generation));
                true
            });
            if !kept {
                // Full: the arrival is refused, about the entry it routed over, and no live unit
                // is dropped to make room.
                out.set(|o| &o.refusal, BUSY);
                out.set(|o| &o.refusal_status, STATUS_BUSY);
                return out.fail(Refusal::refused(BUSY_WORDS));
            }
            // The admission estimate (`admission: estimate`): the least a
            // success bills, one decision and its request fee, so a budget in estimate mode prices
            // a decisions unit before its dial.
            let mut expected = input.units_buf();
            for class in [tail::CLASS_DECISION_INDEX, tail::CLASS_FEE_INDEX] {
                expected.push(UnitCount {
                    class,
                    source: UNITS_ESTIMATED,
                    amount: 1,
                });
            }
            let short = !expected.fits();
            let (written, needed) = expected.settle(short);
            out.set(|o| &o.units_written, written as u32);
            out.set(|o| &o.units_needed, needed as u32);
            if short {
                return Outcome::Failed;
            }
        }
        Outcome::Ready
    }
);

/// `(written, needed)` of every buffer an answer fills, short as a whole when one is short.
fn settle(
    out: &mut Out<'_, OnPieceOut>,
    fields: &HostBuf<'_, OutField>,
    units: &HostBuf<'_, UnitCount>,
    arena: &HostBuf<'_, u8>,
) -> bool {
    let short = !(fields.fits() && units.fits() && arena.fits());
    let (fw, fnd) = fields.settle(short);
    let (uw, und) = units.settle(short);
    let (aw, and) = arena.settle(short);
    out.set(|o| &o.fields_written, fw as u32);
    out.set(|o| &o.fields_needed, fnd as u32);
    out.set(|o| &o.units_written, uw as u32);
    out.set(|o| &o.units_needed, und as u32);
    out.set(|o| &o.arena_written, aw as u64);
    out.set(|o| &o.arena_needed, and as u64);
    short
}

/// Write what `unit` owes into the reply buffer, as much as fits; `more = 1` while any is left,
/// and its flags (with `EMIT_DONE` held back until the last byte) on every write. Answers whether
/// the unit is done.
fn pay(unit: &mut Unit, input: Lent<'_, OnPieceIn>, out: &mut Out<'_, OnPieceOut>) -> bool {
    let n = input.reply_buf().stream(&unit.owed[unit.at..]);
    unit.at += n;
    let more = unit.at < unit.owed.len();
    let done = !more && unit.owed_flags & EMIT_DONE != 0;
    out.set(|o| &o.emitted, n as u64);
    out.set(|o| &o.more, u32::from(more));
    out.set(
        |o| &o.flags,
        if more {
            unit.owed_flags & !EMIT_DONE
        } else {
            unit.owed_flags
        },
    );
    if !more {
        // Paid: the debt's buffer goes now, not when the far end's last piece ends the unit.
        unit.owed = Vec::new();
        unit.at = 0;
    }
    done
}

/// Owe `bytes` under `flags`: write what fits straight from `bytes`, and keep only what does not
/// fit for the re-calls `more = 1` asks for. Answers whether the unit is done.
fn owe(
    unit: &mut Unit,
    bytes: &[u8],
    flags: u32,
    input: Lent<'_, OnPieceIn>,
    out: &mut Out<'_, OnPieceOut>,
) -> bool {
    let n = input.reply_buf().stream(bytes);
    unit.at = 0;
    unit.owed_flags = flags;
    let more = n < bytes.len();
    if more {
        unit.owed.clear();
        unit.owed.extend_from_slice(&bytes[n..]);
    } else {
        // Nothing owed: no buffer is held for it.
        unit.owed = Vec::new();
    }
    out.set(|o| &o.emitted, n as u64);
    out.set(|o| &o.more, u32::from(more));
    out.set(|o| &o.flags, if more { flags & !EMIT_DONE } else { flags });
    !more && flags & EMIT_DONE != 0
}

/// An attempt: the request bound for the far end, its verb, target and dialect fields. A new
/// attempt starts a new far-end answer.
fn attempt_piece(
    unit: &mut Unit,
    input: Lent<'_, OnPieceIn>,
    out: &mut Out<'_, OnPieceOut>,
) -> Outcome {
    let dialect = input.dialect();
    let request = driven::attempt_request(dialect);
    let (mut fields, units, mut arena) = (input.fields_buf(), input.units_buf(), input.arena_buf());
    let verb = arena.span(request.verb.as_bytes());
    let target = arena.span(request.target.as_bytes());
    for (name, value) in request.fields {
        fields.push(OutField {
            name: arena.span(name.as_bytes()),
            value: arena.span(value),
        });
    }
    for (name, value) in unit.caller.iter() {
        fields.push(OutField {
            name: arena.span(name),
            value: arena.span(value),
        });
    }
    if settle(out, &fields, &units, &arena) {
        return Outcome::Failed;
    }
    *unit = Unit {
        attempt: true,
        caller: std::mem::take(&mut unit.caller),
        upstream: unit.upstream.take(),
        routes: unit.routes.take(),
        ticket: unit.ticket,
        owed: std::mem::take(&mut unit.owed),
        reading: FarEndReading::for_dialect(dialect),
        ..Unit::default()
    };
    unit.owed.clear();
    out.set(|o| &o.verb, verb);
    out.set(|o| &o.target, target);
    out.set(|o| &o.flags, EMIT_TO_FAR_END);
    Outcome::Ready
}

/// One piece of the far end's answer: read, and relayed to the caller unchanged with its status
/// and document type on the first piece and its count on the last. Answers the outcome and
/// whether the unit is done.
fn far_end(
    unit: &mut Unit,
    input: Lent<'_, OnPieceIn>,
    out: &mut Out<'_, OnPieceOut>,
) -> (Outcome, bool) {
    let given = input;
    // The far end's trailers (`PIECE_FIELDS`) are no part of its answer: this plane reads no
    // outcome there, so they are neither relayed nor read, and only end the unit when last.
    let trailers = given.flags() & PIECE_FIELDS != 0;
    let bytes = if trailers { &[] } else { input.bytes().bytes() };
    let first = !trailers && given.flags() & PIECE_HAS_STATUS != 0;
    let last = given.flags() & PIECE_LAST != 0;
    if !unit.recall {
        unit.reading.piece(bytes);
        if first {
            unit.status = Some(given.status_code());
        }
    }
    let (mut fields, mut units, mut arena) =
        (input.fields_buf(), input.units_buf(), input.arena_buf());
    if first {
        for f in input.head_fields().iter() {
            let name = f.name().bytes();
            if name.eq_ignore_ascii_case(FIELD_CONTENT_TYPE.as_bytes()) {
                fields.push(OutField {
                    name: arena.span(FIELD_CONTENT_TYPE.as_bytes()),
                    value: arena.span(f.value().bytes()),
                });
            }
        }
    }
    if last {
        // A success bills its decision count, its tokens and its fee. A usage member that is not a
        // whole count bills the plane's floor and is said in one log line, never a silent 0, and
        // the unit that succeeded is not failed. An answer cut short is no delivered reply: only
        // the counts it reported whole before the cut ([`FarEndReading::cut`]).
        let counts = if given.flags() & PIECE_CUT != 0 {
            unit.reading.cut(unit.status)
        } else {
            match unit.reading.counts(unit.status) {
                Ok(counts) => counts,
                Err(driven::UnreadableUsage(pointer)) => {
                    busbar_contract::diag_warn!(
                    busbar_contract::diagnostic::USAGE_TAP_DECODE_FAILED,
                    member = pointer,
                    "the far end's answer has no whole count at `{pointer}`: usage is unreadable; \
                     billing the plane's floor, never 0"
                );
                    unit.reading.floored(unit.status)
                }
            }
        };
        for count in counts.into_iter().flatten() {
            units.push(UnitCount {
                class: count.class,
                source: match count.source {
                    Source::Reported => UNITS_REPORTED,
                    Source::Floor => UNITS_FLOOR,
                },
                amount: count.amount,
            });
        }
    }
    if settle(out, &fields, &units, &arena) {
        unit.recall = true;
        return (Outcome::Failed, false);
    }
    unit.recall = false;
    if first {
        out.set(|o| &o.reply_status, given.status_code());
    }
    let done = owe(unit, bytes, if last { EMIT_DONE } else { 0 }, input, out);
    (Outcome::Ready, done)
}

/// `unit`'s state in `units`, a new entry when it has none; `None` when it has none and
/// [`MAX_UNITS`] live already (a live unit is never dropped to make room).
fn unit_of(units: &mut BTreeMap<u64, Unit>, key: u64) -> Option<&mut Unit> {
    if units.len() >= MAX_UNITS && !units.contains_key(&key) {
        return None;
    }
    Some(units.entry(key).or_default())
}

slot!(
    /// `on_piece`: the ATTEMPT's request head, the caller's body to the far end, and the far
    /// end's answer to the caller. A finished unit is forgotten.
    OnPiece, OnPieceIn, OnPieceOut, |instance, input, mut out| {
        let Some(plane) = instance.get() else {
            return Outcome::Failed;
        };
        let given = input;
        let bytes = input.bytes().bytes();
        plane.units.with_all(|units| {
            // Only a unit `arrive` keeps state for is served: one whose state is gone (cancelled,
            // refused, or never arrived) would cross without its `upstream_model` and its caller
            // fields.
            let Some(unit) = units.get_mut(&given.unit()) else {
                return Outcome::Refused;
            };
            unit.ticket = Some(given.head().ticket());
            // A re-call after `more = 1` carries no bytes, no flags and no attempt: it is paid from
            // what is owed.
            let continues = bytes.is_empty() && given.flags() == 0 && given.attempt_no() == 0;
            let (outcome, done) = if continues && unit.at < unit.owed.len() {
                (Outcome::Ready, pay(unit, input, &mut out))
            } else {
                match given.from() {
                    FROM_KERNEL if given.attempt_no() != 0 => {
                        (attempt_piece(unit, input, &mut out), false)
                    }
                    FROM_CALLER => match driven::caller_piece(bytes, given.flags() & PIECE_LAST != 0) {
                        CallerAnswer::Empty => (Outcome::Refused, true),
                        CallerAnswer::Keep if unit.attempt => {
                            let spliced =
                                unit.upstream.as_deref().and_then(|up| driven::spliced(bytes, up));
                            owe(unit, spliced.as_deref().unwrap_or(bytes), EMIT_TO_FAR_END, input, &mut out);
                            (Outcome::Ready, false)
                        }
                        CallerAnswer::Keep => (Outcome::Ready, false),
                    },
                    FROM_FAR_END => far_end(unit, input, &mut out),
                    _ => (Outcome::Refused, false),
                }
            };
            if done {
                units.remove(&given.unit());
            }
            outcome
        })
    }
);

slot!(
    /// `refusal`: the kernel's status and text in this dialect's error shape, as JSON, and the
    /// wait the kernel handed beside it (`retry_after_s`, a limit's rolling window or the walk's
    /// floor) as `Retry-After`, as every plane answers it.
    RefusalSlot, RefusalIn, RefusalOut, |instance, input, mut out| {
        let given = input;
        // The refusal ends the unit: its state goes with it.
        if let Some(plane) = instance.get() {
            if given.unit() != 0 {
                plane.units.remove(&given.unit());
            }
        }
        let status = u16::try_from(given.status()).unwrap_or(0);
        let text = input.text().as_str().unwrap_or_default();
        let body = driven::refusal_body(status, text);
        let (mut reply, mut fields, mut arena) =
            (input.reply_buf(), input.fields_buf(), input.arena_buf());
        reply.extend(&body);
        fields.push(OutField {
            name: arena.span(FIELD_CONTENT_TYPE.as_bytes()),
            value: arena.span(CONTENT_TYPE_JSON),
        });
        if given.retry_after_s() > 0 {
            let wait = given.retry_after_s().to_string();
            fields.push(OutField {
                name: arena.span(FIELD_RETRY_AFTER.as_bytes()),
                value: arena.span(wait.as_bytes()),
            });
        }
        let short = !(reply.fits() && fields.fits() && arena.fits());
        let (rw, rnd) = reply.settle(short);
        let (fw, fnd) = fields.settle(short);
        let (aw, and) = arena.settle(short);
        out.set(|o| &o.reply_written, rw as u64);
        out.set(|o| &o.reply_needed, rnd as u64);
        out.set(|o| &o.fields_written, fw as u32);
        out.set(|o| &o.fields_needed, fnd as u32);
        out.set(|o| &o.arena_written, aw as u64);
        out.set(|o| &o.arena_needed, and as u64);
        if short {
            Outcome::Failed
        } else {
            Outcome::Ready
        }
    }
);

slot!(
    /// `serve`: the plane publishes no admin route.
    Serve, ServeIn, ServeOut, |_, _, _| { Outcome::Refused }
);

slot!(
    /// `hydrate`: the plane keeps no durable state.
    Hydrate, GenIn, OutHead, |_, _, _| { Outcome::Ready }
);

slot!(
    /// `start`: the plane runs nothing of its own.
    Start, GenIn, OutHead, |_, _, _| { Outcome::Ready }
);

/// A projection that names no body: [`ProjectOut::body`] or [`ProjectOut::rewritten`] absent.
const NO_SPAN: Span = Span {
    offset: SPAN_ABSENT,
    len: 0,
};

slot!(
    /// `project`: the hook view of a unit's request ([`driven::project_hook`]). A request-stage
    /// hook's rewrite is applied first, in the request's dialect ([`driven::rewritten`],
    /// wholesale; a rewrite naming another model is REFUSED, the hook's failure; a rewrite with a
    /// document over a body whose whole document does not parse is FAULT, the kernel's shared
    /// 500): the rewritten request is answered in `rewritten` (the body the kernel keeps and
    /// re-pushes on every attempt from then on) and is the request projected; a rewrite that does
    /// not apply leaves the request as it was. The view's
    /// pool is the entry the request routes over (in the generation `arrive` routed the unit in),
    /// its dialect the claim's, and its one turn the
    /// request document, shown only to a hook granted the prompt. The plane reads nothing of the content
    /// for itself: the view carries the caller's bytes, and no fact is minted from them.
    Project, ProjectIn, ProjectOut, |instance, input, mut out| {
        let Some(plane) = instance.get() else {
            return Outcome::Failed;
        };
        let target = input.target().as_str().unwrap_or_default();
        let Some(dialect) = driven::dialect_at(target) else {
            // A target no claim of this plane names carries no request of this plane's.
            return Outcome::Refused;
        };
        let body = input.body().bytes();
        let rewrite = input.rewrite().bytes();
        // The unit is seen over the generation `arrive` routed it in; one it keeps no state for,
        // over the newest the host holds.
        let generation = plane
            .units
            .with_all(|units| units.get(&input.unit()).and_then(|u| u.routes.clone()))
            .unwrap_or_else(|| plane.routes());
        let routes: &Routes = &generation;
        // The document is checked before any rewrite hook runs, failing closed on a body that
        // would not parse: FAULT, which busbar answers 500 `request rewrite could not be
        // applied`, the body never forwarded and no hook invoked.
        if rewrite.is_empty() && !driven::valid(body) {
            return Outcome::Fault;
        }
        // The request is judged and routed once: a rewrite keeps its `model`, so the entry stands.
        let read = driven::readable(body);
        let mut view = driven::project_read(body, &read, dialect, routes, |r| r.dialect);
        let rewritten = if rewrite.is_empty() {
            None
        } else {
            match driven::rewritten_read(body, &read, rewrite, view.entry) {
                Ok(rewritten) => rewritten,
                // A rewrite naming another model is the hook's failure, never a switch of provider:
                // REFUSED, and the kernel answers it by that hook's `on_error`.
                Err(driven::NotApplied::Model(_)) => return Outcome::Refused,
                // The body's whole document does not parse: the kernel always answers 500 `request
                // rewrite could not be applied`, whatever the hook's `on_error`, and the body is
                // never forwarded.
                Err(driven::NotApplied::Fault) => return Outcome::Fault,
            }
        };
        let request = rewritten.as_deref().unwrap_or(body);
        if let Some(rewritten) = rewritten.as_deref() {
            view.chars = driven::chars_of(rewritten);
        }
        let (signals, mut arena, mut turns) =
            (input.signals_buf(), input.arena_buf(), input.messages_buf());
        let pool = arena.span(view.entry.as_bytes());
        let dialect_name = arena.span(view.dialect.as_bytes());
        let role = arena.span(driven::PROJECTED_ROLE.as_bytes());
        // The request is the projected body and, as text, the one turn: one copy serves both.
        let projected = arena.span(request);
        let text = match std::str::from_utf8(request) {
            Ok(_) => projected,
            Err(_) => arena.span(String::from_utf8_lossy(request).as_bytes()),
        };
        turns.push_turn(&arena, role, text);
        // An applied rewrite IS the projected request: the same bytes, named twice.
        let applied = rewritten.is_some().then_some(projected);
        let short = !(signals.fits() && arena.fits() && turns.fits());
        let (signals_written, signals_needed) = signals.settle(short);
        let (arena_written, arena_needed) = arena.settle(short);
        let (turns_written, turns_needed) = turns.settle(short);
        out.set(|o| &o.signals_needed, signals_needed as u32);
        out.set(|o| &o.arena_written, arena_written as u64);
        out.set(|o| &o.arena_needed, arena_needed as u64);
        out.set(|o| &o.messages_needed, turns_needed as u32);
        out.set(|o| &o.body, NO_SPAN);
        out.set(|o| &o.rewritten, NO_SPAN);
        if short {
            // The driver re-calls once with the buffers this named, the same rewrite with it:
            // applying it again to the same request writes the same body.
            return Outcome::Failed;
        }
        out.host_str(|o| &o.view.pool, &arena, pool);
        out.host_str(|o| &o.view.ingress_dialect, &arena, dialect_name);
        out.set(|o| &o.view.message_count, turns_written as u64);
        out.set(|o| &o.view.total_chars, view.chars as u64);
        out.set(|o| &o.view.flags, 0);
        out.host_rows(|o| &o.view.signals, &signals);
        out.set(|o| &o.view.signals_len, signals_written);
        out.set(|o| &o.prompt.message_count, turns_written as u64);
        out.host_rows(|o| &o.prompt.messages, &turns);
        out.set(|o| &o.prompt.messages_len, turns_written);
        out.set(|o| &o.body, projected);
        if let Some(applied) = applied {
            out.set(|o| &o.rewritten, applied);
        }
        Outcome::Ready
    }
);

busbar_contract::plugin_door! {
    ops: busbar_contract::abi::plane::Ops,
    statement: STATEMENT,
    lifecycle: plane_life(DecisionsDoor),
    kind_ops: {
        arrive: Safe<Arrive>, on_piece: Safe<OnPiece>, refusal: Safe<RefusalSlot>,
        serve: Safe<Serve>, hydrate: Safe<Hydrate>, start: Safe<Start>, project: Safe<Project>,
    },
}

#[cfg(test)]
#[path = "tests/plane_door.rs"]
mod tests;
