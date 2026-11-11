// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! Decisions behavior through its real linked door and independently built image.

use busbar_contract::abi::mechanism::call::{
    AbiStr, Blob, Field, InHead, OutHead, Outcome, BLOB_OCTETS,
};
use busbar_contract::abi::mechanism::lifecycle::{
    slot as life, GenIn, RefreshIn, ReleaseIn, TickIn, TickOut, ValidateIn,
};
use busbar_contract::abi::plane::*;
use busbar_plugin_loader::dispatch::kinds::plane::Plane;
use busbar_plugin_loader::dispatch::{
    in_head, load_dropped, load_linked, out_head, rendering_of, Bind, ConnTable, DispatchConfig,
    Dispatcher, Frame, LinkedRow, NoSink, Plugin,
};
use std::sync::{atomic::Ordering, Arc};

fn zero<T>() -> T {
    // SAFETY: callers instantiate only the plain repr(C) ABI frames/buffers below.
    unsafe { std::mem::zeroed() }
}
fn blob(bytes: &[u8]) -> Blob {
    Blob {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
        fmt: BLOB_OCTETS,
        flags: 0,
    }
}
fn text(bytes: &[u8]) -> AbiStr {
    AbiStr {
        ptr: bytes.as_ptr(),
        len: bytes.len(),
    }
}
const SETTINGS: &[u8] = br#"{"models":{"jev":{"provider":"typesafe","protocol":"jev"}}}"#;
const REQUEST: &[u8] = br#"{"state":{"session":"s"},"context":{}}"#;
const ANSWER: &[u8] = br#"{"request_id":"req_1","usage":{"units":42,"input_tokens":7,"output_tokens":9},"answers":{}}"#;

fn load(dispatcher: &Dispatcher, dropped: bool) -> Plugin<Plane> {
    let bind = Bind {
        instance: Arc::from("decisions"),
        max_inflight_cap: 64,
        sink: Arc::new(NoSink),
        dispatcher: dispatcher.adopter(),
        conns: ConnTable::Probe,
    };
    if dropped {
        let path = std::path::PathBuf::from(
            std::env::var_os("BUSBAR_PLUGIN_CDYLIB")
                .expect("build this plugin and set BUSBAR_PLUGIN_CDYLIB"),
        );
        assert!(
            path.is_absolute() && path.is_file(),
            "an absolute fresh actual plugin image is required: {}",
            path.display()
        );
        let statement = rendering_of(busbar_plane_decisions::door).expect("the actual Statement");
        load_dropped(&path, &statement, bind).expect("actual dropped decisions plugin")
    } else {
        load_linked(
            &LinkedRow::of(busbar_plane_decisions::door).expect("actual linked Statement"),
            bind,
        )
        .expect("actual linked decisions plugin")
    }
}
fn first(p: &Plugin<Plane>) -> u64 {
    assert_eq!(
        p.test_invocations().1,
        0,
        "decisions synchronous cases never resume"
    );
    p.test_invocations().0
}
fn called(c: &busbar_plugin_loader::dispatch::Called) -> String {
    format!("{:?} lease={} error={:?}", c.outcome, c.lease != 0, c.error)
}
extern "C" fn actual_door() -> *const busbar_contract::abi::mechanism::door::Door {
    busbar_plane_decisions::door()
}

fn arrive(
    p: &Plugin<Plane>,
    observed: &mut Vec<String>,
    unit: u64,
    method: &[u8],
    target: &[u8],
    body: &[u8],
) -> (Outcome, u32) {
    let mut f: Frame<ArriveIn, ArriveOut> = Frame::new(zero(), zero());
    f.input.head = in_head();
    f.out.head = out_head();
    f.input.unit = unit;
    f.input.method = text(method);
    f.input.target = text(target);
    f.input.body = blob(body);
    let mut units: [UnitCount; 4] = [zero(); 4];
    f.input.units_buf = units.as_mut_ptr();
    f.input.units_cap = units.len();
    let called = p.call(slot::ARRIVE, &mut f);
    observed.push(format!(
        "arrive {} op_class={} principal={} dialect={} units={} refusal={} status={}",
        crate::called(&called),
        f.out.op_class,
        f.out.principal_need,
        f.out.dialect,
        f.out.units_written,
        f.out.refusal,
        f.out.refusal_status
    ));
    (called.outcome, f.out.refusal_status)
}

#[derive(Debug, PartialEq, Eq)]
struct Answer {
    outcome: Outcome,
    emitted: Vec<u8>,
    units: Vec<(u32, u64, u32)>,
    more: u32,
    done: bool,
    to_far: bool,
    status: u32,
    call: String,
    verb: Vec<u8>,
    target: Vec<u8>,
    fields: Vec<(Vec<u8>, Vec<u8>)>,
    lane: Vec<u8>,
    records: Vec<(u32, u32, Vec<u8>, Vec<u8>)>,
}
struct Piece {
    reply: Vec<u8>,
    units: [UnitCount; 8],
    fields: [OutField; 8],
    records: [RecordWrite; 8],
    arena: [u8; 1024],
}
impl Piece {
    fn new(cap: usize) -> Self {
        Self {
            reply: vec![0; cap],
            units: [zero(); 8],
            fields: [zero(); 8],
            records: [zero(); 8],
            arena: [0; 1024],
        }
    }
    fn frame(
        &mut self,
        unit: u64,
        from: u32,
        flags: u32,
        bytes: &[u8],
        units_cap: usize,
    ) -> Frame<OnPieceIn, OnPieceOut> {
        let mut f: Frame<OnPieceIn, OnPieceOut> = Frame::new(zero(), zero());
        f.input.head = in_head();
        f.out.head = out_head();
        f.input.unit = unit;
        f.input.from = from;
        f.input.flags = flags;
        f.input.bytes = blob(bytes);
        f.input.member = text(b"jev");
        f.input.attempt_no = u32::from(from == FROM_KERNEL);
        const HEAD: [Field; 1] = [Field {
            name: AbiStr {
                ptr: b"content-type".as_ptr(),
                len: 12,
            },
            value: AbiStr {
                ptr: b"application/json".as_ptr(),
                len: 16,
            },
        }];
        if flags & PIECE_HAS_STATUS != 0 {
            f.input.head_fields = HEAD.as_ptr();
            f.input.head_fields_len = HEAD.len();
        }
        f.input.status_code = if flags & PIECE_HAS_STATUS != 0 {
            200
        } else {
            0
        };
        f.input.reply_buf = self.reply.as_mut_ptr();
        f.input.reply_cap = self.reply.len();
        f.input.units_buf = self.units.as_mut_ptr();
        f.input.units_cap = units_cap;
        f.input.fields_buf = self.fields.as_mut_ptr();
        f.input.fields_cap = self.fields.len();
        f.input.records_buf = self.records.as_mut_ptr();
        f.input.records_cap = self.records.len();
        f.input.arena_buf = self.arena.as_mut_ptr();
        f.input.arena_cap = self.arena.len();
        f
    }
    fn read(&self, result: &busbar_plugin_loader::dispatch::Called, out: &OnPieceOut) -> Answer {
        let at = |span: busbar_contract::abi::mechanism::call::Span| {
            self.arena[span.offset as usize..(span.offset + span.len) as usize].to_vec()
        };
        Answer {
            outcome: result.outcome,
            emitted: self.reply[..out.emitted as usize].to_vec(),
            units: self.units[..out.units_written as usize]
                .iter()
                .map(|u| (u.class, u.amount, u.source))
                .collect(),
            more: out.more,
            done: out.flags & EMIT_DONE != 0,
            to_far: out.flags & EMIT_TO_FAR_END != 0,
            status: out.reply_status,
            call: called(result),
            verb: at(out.verb),
            target: at(out.target),
            fields: self.fields[..out.fields_written as usize]
                .iter()
                .map(|f| (at(f.name), at(f.value)))
                .collect(),
            lane: at(out.lane),
            records: self.records[..out.records_written as usize]
                .iter()
                .map(|r| (r.op, r.kind, at(r.key), at(r.value)))
                .collect(),
        }
    }
    fn call(
        &mut self,
        p: &Plugin<Plane>,
        unit: u64,
        from: u32,
        flags: u32,
        bytes: &[u8],
    ) -> Answer {
        let mut frame = self.frame(unit, from, flags, bytes, self.units.len());
        let before = first(p);
        let called = p.call(slot::ON_PIECE, &mut frame);
        assert_eq!(
            first(p) - before,
            1,
            "one first invocation per decisions piece"
        );
        self.read(&called, &frame.out)
    }
}

#[test]
fn decisions_lifecycle_requests_and_short_buffers_match_both_real_doors() {
    assert!(
        !(cfg!(debug_assertions) && std::env::var_os("BUSBAR_EXPECT_RELEASE").is_some()),
        "BUSBAR_EXPECT_RELEASE requires --release"
    );
    let mut legs = Vec::new();
    for dropped in [false, true] {
        let dispatcher = Dispatcher::new(
            DispatchConfig::default(),
            busbar_contract::services::test_caller_binding(),
        );
        let mut observed = Vec::new();
        let p = load(&dispatcher, dropped);
        assert_eq!(p.kind(), busbar_contract::abi::mechanism::KindCode::Plane);
        assert_eq!(p.max_inflight(), 64);
        observed.push(format!(
            "facts {:?} {} max_inflight={}",
            p.kind(),
            p.name(),
            p.max_inflight()
        ));
        assert_eq!(p.served().billable_classes.len(), 4);
        let before = first(&p);
        let mut bad: Frame<ValidateIn, OutHead> = Frame::new(zero(), out_head());
        bad.input.head = in_head();
        bad.input.settings = blob(br#"{"decisions":{"modles":{}}}"#);
        let refused = p.call(life::VALIDATE, &mut bad);
        assert_eq!(refused.outcome, Outcome::Refused);
        assert!(
            refused.error.as_ref().is_some_and(|e| !e.is_empty()),
            "bad settings name their reason"
        );
        observed.push(format!("validate bad {}", called(&refused)));
        assert_eq!(first(&p) - before, 1);
        let before = first(&p);
        bad.input.settings =
            blob(br#"{"decisions":{"models":{"jev":{"provider":"typesafe","protocol":"jev"}}}}"#);
        let valid = p.call(life::VALIDATE, &mut bad);
        assert_eq!(valid.outcome, Outcome::Ready);
        observed.push(format!("validate {}", called(&valid)));
        assert_eq!(first(&p) - before, 1);
        let before = first(&p);
        assert_eq!(
            arrive(&p, &mut observed, 7, b"POST", b"/v1/systemone", REQUEST).0,
            Outcome::Refused
        );
        assert_eq!(first(&p) - before, 0, "unopened request never crosses");
        let mut open: Frame<PlaneOpenIn, PlaneOpenOut> = Frame::new(zero(), zero());
        open.input.open.head = in_head();
        open.out.open.head = out_head();
        open.input.open.generation = 1;
        open.input.open.settings = blob(SETTINGS);
        let before = first(&p);
        let (called, snapshot) = p.open(&mut open);
        assert_eq!(called.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1);
        observed.push(format!("open {} {snapshot:?}", crate::called(&called)));
        let snapshot = snapshot.expect("owned snapshot");
        assert_eq!(snapshot.generation, 1);
        assert_eq!(
            snapshot
                .claims
                .iter()
                .map(|c| (c.verb.as_str(), c.target.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("POST", "/v1/systemone"),
                ("POST", "/busbar/decisions/v1/classify")
            ]
        );
        let ready_pin = u64::from(p.has_ready());
        let before = first(&p);
        p.ready(&dispatcher, std::time::Duration::from_secs(30))
            .expect("actual ready");
        assert_eq!(first(&p) - before, ready_pin, "ready exact crossings");
        observed.push(format!("ready {ready_pin}"));
        let before = first(&p);
        let reopened = p.open(&mut open).0;
        assert_eq!(reopened.outcome, Outcome::Refused);
        observed.push(format!("open again {}", crate::called(&reopened)));
        assert_eq!(first(&p) - before, 0, "duplicate open never crosses");
        for slot in [slot::HYDRATE, slot::START] {
            let mut f = Frame::new(
                GenIn {
                    head: in_head(),
                    generation: 1,
                },
                out_head(),
            );
            let before = first(&p);
            let life = p.call(slot, &mut f);
            assert_eq!(life.outcome, Outcome::Ready);
            observed.push(format!("lifecycle {slot} {}", crate::called(&life)));
            assert_eq!(first(&p) - before, 1);
        }
        let mut answers = Vec::new();
        for (unit, cap) in [(7, 1024), (9, 40), (11, 1024)] {
            let before = first(&p);
            assert_eq!(
                arrive(&p, &mut observed, unit, b"POST", b"/v1/systemone", REQUEST).0,
                Outcome::Ready
            );
            assert_eq!(first(&p) - before, 1);
            let mut buffers = Piece::new(cap);
            let attempt = buffers.call(&p, unit, FROM_KERNEL, 0, b"");
            observed.push(format!("attempt {attempt:?}"));
            assert_eq!(attempt.outcome, Outcome::Ready);
            assert!(attempt.to_far);
            let body = buffers.call(&p, unit, FROM_CALLER, PIECE_LAST, REQUEST);
            observed.push(format!("body {body:?}"));
            assert_eq!(body.outcome, Outcome::Ready);
            assert_eq!(body.more, 0);
            assert_eq!(body.emitted, REQUEST);
            assert!(body.to_far);
            let answer = if unit == 11 {
                let mut frame =
                    buffers.frame(unit, FROM_FAR_END, PIECE_HAS_STATUS | PIECE_LAST, ANSWER, 0);
                let before = first(&p);
                let short = p.call(slot::ON_PIECE, &mut frame);
                assert_eq!(short.outcome, Outcome::Failed);
                assert!(frame.out.units_needed > 0);
                observed.push(format!(
                    "short {} units_needed={} fields_needed={} arena_needed={}",
                    crate::called(&short),
                    frame.out.units_needed,
                    frame.out.fields_needed,
                    frame.out.arena_needed
                ));
                let recall = short.recall.expect("short result has one recall");
                frame.input.units_cap = 8;
                frame.out = zero();
                frame.out.head = out_head();
                let called = p.recall(recall, slot::ON_PIECE, &mut frame);
                assert_eq!(
                    first(&p) - before,
                    2,
                    "short and exactly one recalled crossing"
                );
                buffers.read(&called, &frame.out)
            } else {
                buffers.call(
                    &p,
                    unit,
                    FROM_FAR_END,
                    PIECE_HAS_STATUS | PIECE_LAST,
                    ANSWER,
                )
            };
            observed.push(format!("far end {answer:?}"));
            assert_eq!(answer.outcome, Outcome::Ready);
            assert_eq!(answer.status, 200);
            assert!(!answer.to_far);
            assert_eq!(
                answer.units,
                vec![
                    (0, 42, UNITS_REPORTED),
                    (2, 7, UNITS_REPORTED),
                    (3, 9, UNITS_REPORTED),
                    (1, 1, UNITS_REPORTED)
                ]
            );
            let mut emitted = answer.emitted.clone();
            if unit == 9 {
                assert_eq!(answer.more, 1);
                assert!(!answer.done);
                for index in 0..2 {
                    let more = buffers.call(&p, unit, FROM_FAR_END, 0, b"");
                    observed.push(format!("more {more:?}"));
                    assert!(!more.to_far);
                    assert_eq!(more.outcome, Outcome::Ready);
                    assert_eq!(more.more, u32::from(index == 0));
                    assert_eq!(more.done, index == 1);
                    emitted.extend_from_slice(&more.emitted);
                }
            } else {
                assert_eq!(answer.more, 0);
                assert!(answer.done);
            }
            assert_eq!(emitted, ANSWER);
            answers.push(emitted);
        }
        let before = first(&p);
        assert_eq!(
            arrive(&p, &mut observed, 8, b"GET", b"/v1/models", REQUEST),
            (Outcome::Refused, 404)
        );
        assert_eq!(first(&p) - before, 1);
        let before = first(&p);
        assert_eq!(
            arrive(&p, &mut observed, 10, b"POST", b"/v1/systemone", REQUEST).0,
            Outcome::Ready
        );
        assert_eq!(first(&p) - before, 1, "empty arrive exact crossings");
        let empty = Piece::new(1024).call(&p, 10, FROM_CALLER, PIECE_LAST, b"");
        assert_eq!(empty.outcome, Outcome::Refused);
        observed.push(format!("empty {empty:?}"));
        let mut reply = [0; 512];
        let mut arena = [0; 256];
        let mut fields: [OutField; 2] = [zero(); 2];
        let mut refusal: Frame<RefusalIn, RefusalOut> = Frame::new(zero(), zero());
        refusal.input.head = in_head();
        refusal.out.head = out_head();
        refusal.input.cause = REFUSAL_GATE;
        refusal.input.status = 403;
        refusal.input.text = text(b"denied");
        refusal.input.reply_buf = reply.as_mut_ptr();
        refusal.input.reply_cap = reply.len();
        refusal.input.arena_buf = arena.as_mut_ptr();
        refusal.input.arena_cap = arena.len();
        refusal.input.fields_buf = fields.as_mut_ptr();
        refusal.input.fields_cap = fields.len();
        let before = first(&p);
        let result = p.call(slot::REFUSAL, &mut refusal);
        assert_eq!(result.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1, "refusal exact crossings");
        observed.push(format!(
            "refusal {} {}",
            crate::called(&result),
            format_args!(
                "status={} reply={:?} fields={:?}",
                refusal.out.status,
                &reply[..refusal.out.reply_written as usize],
                fields[..refusal.out.fields_written as usize]
                    .iter()
                    .map(|f| (
                        arena[f.name.offset as usize..(f.name.offset + f.name.len) as usize]
                            .to_vec(),
                        arena[f.value.offset as usize..(f.value.offset + f.value.len) as usize]
                            .to_vec()
                    ))
                    .collect::<Vec<_>>()
            )
        ));
        assert_eq!(
            &reply[..refusal.out.reply_written as usize],
            br#"{"error":{"code":"unsupported_operation","message":"denied"}}"#
        );
        let mut serve: Frame<ServeIn, ServeOut> = Frame::new(zero(), zero());
        serve.input.head = in_head();
        serve.out.head = out_head();
        let before = first(&p);
        let result = p.call(slot::SERVE, &mut serve);
        assert_eq!(result.outcome, Outcome::Refused);
        assert_eq!(first(&p) - before, 1, "serve exact crossings");
        observed.push(format!(
            "serve {} {}",
            crate::called(&result),
            format_args!(
                "status={} written={}",
                serve.out.status, serve.out.reply_written
            )
        ));
        let mut project: Frame<ProjectIn, ProjectOut> = Frame::new(zero(), zero());
        project.input.head = in_head();
        project.out.head = out_head();
        project.input.body = blob(REQUEST);
        project.input.target = text(b"/v1/systemone");
        let mut signals = [zero::<busbar_contract::abi::hook::SignalEntry>(); 4];
        let mut turns = [zero::<busbar_contract::abi::hook::MessageView>(); 4];
        project.input.signals_buf = signals.as_mut_ptr();
        project.input.signals_cap = signals.len();
        project.input.messages_buf = turns.as_mut_ptr();
        project.input.messages_cap = turns.len();
        project.input.arena_buf = arena.as_mut_ptr();
        project.input.arena_cap = arena.len();
        let before = first(&p);
        let result = p.call(slot::PROJECT, &mut project);
        assert_eq!(result.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1, "project exact crossings");
        observed.push(format!(
            "project {} {}",
            crate::called(&result),
            format_args!("signals={}", project.out.view.signals_len)
        ));
        let mut drive: Frame<PlaneDriveIn, PlaneDriveOut> = Frame::new(zero(), zero());
        drive.input.drive.head = in_head();
        drive.out.head = out_head();
        let before = first(&p);
        let result = p.call(life::DRIVE, &mut drive);
        assert_eq!(result.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1, "drive exact crossings");
        observed.push(format!(
            "drive {} {}",
            crate::called(&result),
            format_args!("sessions={}", drive.out.sessions_written)
        ));
        let mut release = Frame::new(
            ReleaseIn {
                head: in_head(),
                lease: 0,
            },
            out_head(),
        );
        let before = first(&p);
        let result = p.call(life::RELEASE, &mut release);
        assert_eq!(first(&p) - before, 1, "release exact crossings");
        assert_eq!(result.lease, 0);
        assert!(
            result.outcome == Outcome::Ready && result.error.as_ref().is_none_or(|e| e.is_empty())
                || result.outcome == Outcome::Refused
                    && result.error.as_deref() == Some(b"release: no such lease")
        );
        observed.push(format!("release {}", crate::called(&result)));
        let mut tick = Frame::new(
            TickIn {
                head: in_head(),
                now_ns: 1000,
            },
            TickOut {
                head: out_head(),
                next_tick_ns: 0,
            },
        );
        let before = first(&p);
        let result = p.call(life::TICK, &mut tick);
        assert_eq!(result.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1, "tick exact crossings");
        observed.push(format!(
            "tick {} {}",
            crate::called(&result),
            format_args!("next={}", tick.out.next_tick_ns)
        ));
        let mut cancel: Frame<PlaneCancelIn, PlaneCancelOut> = Frame::new(zero(), zero());
        cancel.input.cancel.head = in_head();
        cancel.out.cancel.head = out_head();
        let before = first(&p);
        let result = p.call(life::CANCEL, &mut cancel);
        assert_eq!(result.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1, "cancel exact crossings");
        observed.push(format!(
            "cancel {} {}",
            crate::called(&result),
            format_args!("disposition={}", cancel.out.cancel.disposition)
        ));
        let mut refresh: Frame<RefreshIn, PlaneRefreshOut> = Frame::new(zero(), zero());
        refresh.input.head = in_head();
        refresh.out.head = out_head();
        refresh.input.generation = 2;
        refresh.input.settings=blob(br#"{"models":{"a":{"provider":"typesafe","protocol":"jev"},"b":{"provider":"typesafe","protocol":"jev"}}}"#);
        let before = first(&p);
        let (called, snapshot) = p.refresh(&mut refresh);
        assert_eq!(first(&p) - before, 1, "refresh exact crossings");
        observed.push(format!("refresh {} {snapshot:?}", crate::called(&called)));
        assert_eq!(called.outcome, Outcome::Ready);
        let snapshot = snapshot.expect("refresh snapshot");
        assert_eq!(snapshot.generation, 2);
        assert_eq!(
            snapshot
                .claims
                .iter()
                .map(|c| (c.verb.as_str(), c.target.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("POST", "/v1/systemone"),
                ("POST", "/busbar/decisions/v1/classify")
            ]
        );
        let mut retire = Frame::new(
            GenIn {
                head: in_head(),
                generation: 1,
            },
            out_head(),
        );
        let before = first(&p);
        let result = p.call(life::RETIRE, &mut retire);
        assert_eq!(result.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1, "retire exact crossings");
        observed.push(format!(
            "retire {} {}",
            crate::called(&result),
            String::new()
        ));
        let mut close: Frame<InHead, OutHead> = Frame::new(in_head(), out_head());
        let before = first(&p);
        let result = p.call(life::CLOSE, &mut close);
        assert_eq!(result.outcome, Outcome::Ready);
        assert_eq!(first(&p) - before, 1, "close exact crossings");
        observed.push(format!(
            "close {} {}",
            crate::called(&result),
            String::new()
        ));
        let before = first(&p);
        assert_eq!(
            arrive(&p, &mut observed, 7, b"POST", b"/v1/systemone", REQUEST).0,
            Outcome::Fault
        );
        assert_eq!(first(&p) - before, 0);
        legs.push((observed, answers));
    }
    assert_eq!(legs[0], legs[1]);
}

// One malformed-metadata input: actual decisions callbacks are retained unchanged.
// The one test below owns this slot; no valid replacement plugin is implemented.
static MALFORMED: std::sync::atomic::AtomicPtr<busbar_contract::abi::mechanism::door::Door> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
extern "C" fn malformed_door() -> *const busbar_contract::abi::mechanism::door::Door {
    MALFORMED.load(Ordering::SeqCst)
}

#[test]
fn incompatible_decisions_metadata_is_refused_by_both_loader_paths() {
    use busbar_contract::abi::mechanism::door::{Door, DoorParts, KindTailHead, Statement};
    use busbar_contract::abi::mechanism::{rendering::RENDERING_MAGIC, KindCode};
    use busbar_plugin_loader::dispatch::load::rendering_of_raw;
    use busbar_plugin_loader::dispatch::{Kind, LoadError};
    let dispatcher = Dispatcher::new(
        DispatchConfig::default(),
        busbar_contract::services::test_caller_binding(),
    );
    let bind = || Bind {
        instance: Arc::from("decisions-negative"),
        max_inflight_cap: 64,
        sink: Arc::new(NoSink),
        dispatcher: dispatcher.adopter(),
        conns: ConnTable::Probe,
    };
    let stated = rendering_of(busbar_plane_decisions::door).expect("actual Statement");
    let path = std::path::PathBuf::from(
        std::env::var_os("BUSBAR_PLUGIN_CDYLIB").expect("actual built plugin"),
    );
    assert!(path.is_absolute() && path.is_file());
    load_linked::<Plane>(
        &LinkedRow::of(busbar_plane_decisions::door).unwrap(),
        bind(),
    )
    .expect("honest linked admission");
    load_dropped::<Plane>(&path, &stated, bind()).expect("honest dropped admission");
    let (image_door, image_ops) =
        busbar_plugin_loader::dispatch::load::door_addresses_of_library(&path)
            .expect("the actual library loads")
            .expect("the actual library exports its door");
    let actual = busbar_plane_decisions::door();
    assert_ne!(
        image_door, actual as *const Door as usize,
        "the dropped door is a distinct image"
    );
    assert_ne!(
        image_ops,
        actual.ops() as usize,
        "the dropped callbacks are independent of linked code"
    );
    let read =
        busbar_contract::abi::mechanism::rendering::read(&stated).expect("actual declarations");
    assert_eq!(read.needs.len(), 1, "one actual HTTP bearer need");
    assert_eq!(read.needs[0].transport, "http");
    assert_eq!(read.needs[0].auth, "bearer");
    let expected = LoadError::NoConnectionTable {
        plugin: read.name.clone(),
        needs: 1,
    };
    let row = LinkedRow::of(busbar_plane_decisions::door).unwrap();
    let serving = || Bind {
        conns: ConnTable::NoNeeds,
        ..bind()
    };
    let linked = load_linked::<Plane>(&row, serving()).expect_err("real serving needs its table");
    let dropped = load_dropped::<Plane>(&path, &stated, serving())
        .expect_err("real dropped serving needs its table");
    assert_eq!(linked, expected);
    assert_eq!(dropped, expected);
    assert!(linked
        .to_string()
        .contains(&format!("plugin '{}'", read.name)));
    load_linked::<Plane>(&row, bind()).expect("Probe permits binding without serving connections");
    load_dropped::<Plane>(&path, &stated, bind()).expect("dropped Probe permits binding");
    // SAFETY: the actual static product door owns this complete metadata and its callbacks.
    let real = unsafe { Door::read_parts(busbar_plane_decisions::door()) };
    let publish = |parts: DoorParts| {
        // SAFETY: only ABI metadata is perturbed; all pointed-to storage is leaked/static
        // and actual callbacks are never called for a rejected preamble.
        let door = unsafe { Door::__from_parts(parts) };
        MALFORMED.store(Box::into_raw(Box::new(door)), Ordering::SeqCst);
    };
    for version in [real.kind_abi.wrapping_sub(1), real.kind_abi + 1] {
        publish(DoorParts {
            kind_abi: version,
            ..real
        });
        let want = LoadError::KindAbi {
            kind: KindCode::Plane,
            door: version,
            host: real.kind_abi,
        };
        assert_eq!(rendering_of_raw(malformed_door).err(), Some(want.clone()));
        assert_eq!(
            LinkedRow::of_raw(malformed_door)
                .and_then(|row| load_linked::<Plane>(&row, bind()).map(|_| ()))
                .err(),
            Some(want)
        );
        let mut manifest = stated.clone();
        let at = RENDERING_MAGIC.len() + 8;
        manifest[at..at + 4].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            load_dropped::<Plane>(&path, &manifest, bind()).err(),
            Some(LoadError::ManifestKindAbi {
                stated: version,
                host: real.kind_abi
            })
        );
    }
    for layout in [0, real.layout ^ 1] {
        publish(DoorParts { layout, ..real });
        let error =
            rendering_of_raw(malformed_door).expect_err("wrong kind layout cannot be packed");
        assert!(
            matches!(error, LoadError::KindLayout { stated, host, .. } if stated == layout && host == real.layout)
        );
        assert!(error
            .to_string()
            .contains("rebuild against this busbar-contract"));
        let row = LinkedRow {
            statement: stated.clone(),
            door: malformed_door,
        };
        assert_eq!(load_linked::<Plane>(&row, bind()).err(), Some(error));
    }
    // SAFETY: the actual admitted product Statement/tail are complete static ABI values.
    let statement = unsafe { real.statement.read_unaligned() };
    let tail = unsafe { statement.kind_tail.cast::<PlaneTail>().read_unaligned() };
    let tail = Box::leak(Box::new(PlaneTail {
        answer_layout: ANSWER_LAYOUT - 1,
        ..tail
    }));
    let changed = Box::leak(Box::new(Statement {
        kind_tail: std::ptr::from_ref(tail).cast::<KindTailHead>(),
        ..statement
    }));
    publish(DoorParts {
        statement: changed,
        ..real
    });
    let error =
        rendering_of_raw(malformed_door).expect_err("wrong plane answer layout cannot be packed");
    assert!(
        matches!(error, LoadError::PlaneLayout { stated, host, .. } if stated == ANSWER_LAYOUT - 1 && host == ANSWER_LAYOUT)
    );
    assert_eq!(
        LinkedRow::of_raw(malformed_door)
            .and_then(|row| load_linked::<Plane>(&row, bind()).map(|_| ()))
            .err(),
        Some(error)
    );
    let changed = Box::leak(Box::new(Statement {
        version: text(b"0.0.0-invalid-declaration"),
        ..statement
    }));
    publish(DoorParts {
        statement: changed,
        ..real
    });
    let lying = rendering_of_raw(malformed_door).expect("different valid Statement rendering");
    assert_ne!(lying, stated);
    let row = LinkedRow {
        statement: lying.clone(),
        door: actual_door,
    };
    assert_eq!(
        load_linked::<Plane>(&row, bind()).err(),
        Some(LoadError::StatementMismatch)
    );
    assert_eq!(
        load_dropped::<Plane>(&path, &lying, bind()).err(),
        Some(LoadError::StatementMismatch)
    );
    let row = LinkedRow {
        statement: stated.clone(),
        door: malformed_door,
    };
    assert_eq!(
        load_linked::<Plane>(&row, bind()).err(),
        Some(LoadError::StatementMismatch)
    );
    publish(DoorParts {
        ready: Some(failing_ready),
        ..real
    });
    READY_CALLS.store(0, Ordering::SeqCst);
    let row = LinkedRow::of_raw(malformed_door).expect("otherwise actual product metadata");
    let ready_plugin =
        load_linked::<Plane>(&row, bind()).expect("the actual product with failing READY loads");
    let mut open: Frame<PlaneOpenIn, PlaneOpenOut> = Frame::new(zero(), zero());
    open.input.open.head = in_head();
    open.out.open.head = out_head();
    open.input.open.generation = 1;
    open.input.open.settings = blob(SETTINGS);
    assert_eq!(ready_plugin.open(&mut open).0.outcome, Outcome::Ready);
    let before_ready = first(&ready_plugin);
    let refused = ready_plugin
        .ready(&dispatcher, std::time::Duration::from_secs(30))
        .expect_err("failing READY refuses boot");
    assert_eq!(
        refused,
        format!(
            "plugin '{}' ready failed: decisions-test: ready refused",
            ready_plugin.name()
        )
    );
    assert_eq!(
        first(&ready_plugin) - before_ready,
        1,
        "failing READY has one crossing, never retried"
    );
    assert_eq!(
        READY_CALLS.load(Ordering::SeqCst),
        1,
        "READY is awaited exactly once"
    );
    let row = LinkedRow::of(busbar_plane_decisions::door).unwrap();
    use busbar_plugin_loader::dispatch::kinds::{
        auth::Auth, export::Export, hook::Hook, secret::Secret, store_v4::StoreV4 as Store,
        transport::Transport,
    };
    // Explicit wrong-kind tests use the real product image under each loader kind.
    let want = |kind| {
        Some(LoadError::ManifestKind {
            stated: KindCode::Plane,
            want: kind,
        })
    };
    assert_eq!(load_linked::<Auth>(&row, bind()).err(), want(Auth::CODE));
    assert_eq!(
        load_dropped::<Auth>(&path, &stated, bind()).err(),
        want(Auth::CODE)
    );
    assert_eq!(
        load_linked::<Export>(&row, bind()).err(),
        want(Export::CODE)
    );
    assert_eq!(
        load_dropped::<Export>(&path, &stated, bind()).err(),
        want(Export::CODE)
    );
    assert_eq!(load_linked::<Hook>(&row, bind()).err(), want(Hook::CODE));
    assert_eq!(
        load_dropped::<Hook>(&path, &stated, bind()).err(),
        want(Hook::CODE)
    );
    assert_eq!(
        load_linked::<Secret>(&row, bind()).err(),
        want(Secret::CODE)
    );
    assert_eq!(
        load_dropped::<Secret>(&path, &stated, bind()).err(),
        want(Secret::CODE)
    );
    assert_eq!(load_linked::<Store>(&row, bind()).err(), want(Store::CODE));
    assert_eq!(
        load_dropped::<Store>(&path, &stated, bind()).err(),
        want(Store::CODE)
    );
    assert_eq!(
        load_linked::<Transport>(&row, bind()).err(),
        want(Transport::CODE)
    );
    assert_eq!(
        load_dropped::<Transport>(&path, &stated, bind()).err(),
        want(Transport::CODE)
    );
}

static READY_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
extern "C" fn failing_ready(
    _: *mut std::ffi::c_void,
    _: *const std::ffi::c_void,
    out: *mut std::ffi::c_void,
) -> busbar_contract::abi::mechanism::call::RawOutcome {
    use busbar_contract::abi::mechanism::call::RawOutcome;
    READY_CALLS.fetch_add(1, Ordering::SeqCst);
    let failed = RawOutcome::of(Outcome::Failed);
    // SAFETY: the dispatcher supplies a live complete OutHead for READY. This tiny malformed
    // lifecycle input writes only its outcome and static failure reason, never product logic.
    unsafe {
        let head = &mut *out.cast::<OutHead>();
        head.outcome = failed;
        head.error = text(b"decisions-test: ready refused");
    }
    failed
}
