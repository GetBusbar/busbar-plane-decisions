// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DOOR, BOTH WAYS: the logic crate's `door` linked and this crate's dropped-in image (the
//! `busbar-plane-decisions-plugin` cdylib `cargo test` builds beside the test), each loaded through
//! busbar's real loader and driven alike. The published suite's both-ways fold over
//! `conformance.json` is `tests/conformance.rs`; this file holds what that suite does not script.
//!
//! THE PLANE'S LOG REACHES THE HOST'S LOG, DROPPED IN AS LINKED (owner ruling 2026-10-27). The
//! line an unreadable usage raises (`busbar_contract::diag_warn!`, the plane's in-process
//! `tracing`) is carried by the door's call capture in the answer's envelope to the host's sink,
//! the one that writes the plugin's own log file.

use std::mem::zeroed;
use std::sync::{Arc, Mutex, PoisonError};

use busbar_contract::abi::mechanism::call::{AbiStr, Blob, Outcome, BLOB_OCTETS};
use busbar_contract::abi::plane::{
    slot, ArriveIn, ArriveOut, OnPieceIn, OnPieceOut, OutField, PlaneOpenIn, PlaneOpenOut,
    ProviderFacts, UnitCount, FROM_CALLER, FROM_FAR_END, FROM_KERNEL, PIECE_HAS_STATUS, PIECE_LAST,
    UNITS_FLOOR,
};
use busbar_plugin_loader::dispatch::kinds::plane::Plane;
use busbar_plugin_loader::dispatch::{
    in_head, load_dropped, load_linked, out_head, rendering_of, Bind, Diagnostic, DispatchConfig,
    Dispatcher, Dropped, EnvelopeSink, Frame, LinkedRow, Metric, Plugin,
};

fn z<T>() -> T {
    // SAFETY: every `in`/`out` here is plain C data; all-zero is a valid value of each.
    unsafe { zeroed() }
}

fn octets(b: &'static [u8]) -> Blob {
    Blob {
        ptr: b.as_ptr(),
        len: b.len(),
        fmt: BLOB_OCTETS,
        flags: 0,
    }
}

const fn text(b: &'static [u8]) -> AbiStr {
    AbiStr {
        ptr: b.as_ptr(),
        len: b.len(),
    }
}

/// The host's envelope sink, keeping the text of every diagnostic and log record a call carried.
#[derive(Default)]
struct HostLog(Mutex<Vec<String>>);

impl EnvelopeSink for HostLog {
    fn metric(&self, _: Metric<'_>) {}
    fn diag(&self, d: Diagnostic<'_>) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(String::from_utf8_lossy(d.text).into_owned());
    }
    fn dropped(&self, _: Dropped) {}
}

/// One model, served by provider `typesafe` (protocol `jev`).
const SETTINGS: &[u8] = br#"{"models":{"jev":{"provider":"typesafe"}}}"#;
/// The caller's request.
const BODY: &[u8] = br#"{"state":{}}"#;
/// A success whose decision count is not a whole count.
const UNREADABLE: &[u8] = br#"{"usage":{"units":"7"}}"#;

/// One piece of `unit`, answered: the outcome and the units it wrote, `(class, source, amount)`.
fn piece(
    p: &Plugin<Plane>,
    unit: u64,
    from: u32,
    flags: u32,
    status: u32,
    bytes: &'static [u8],
    attempt_no: u32,
) -> (Outcome, Vec<(u32, u32, u64)>) {
    let mut reply = vec![0_u8; 1 << 12];
    let mut units = [z::<UnitCount>(); 8];
    let mut fields = [z::<OutField>(); 8];
    let mut arena = [0_u8; 1024];
    let mut i: OnPieceIn = z();
    i.head = in_head();
    (i.unit, i.from, i.flags, i.bytes) = (unit, from, flags, octets(bytes));
    (i.status_code, i.attempt_no) = (status, attempt_no);
    (i.reply_buf, i.reply_cap) = (reply.as_mut_ptr(), reply.len());
    (i.units_buf, i.units_cap) = (units.as_mut_ptr(), units.len());
    (i.fields_buf, i.fields_cap) = (fields.as_mut_ptr(), fields.len());
    (i.arena_buf, i.arena_cap) = (arena.as_mut_ptr(), arena.len());
    let mut o: OnPieceOut = z();
    o.head = out_head();
    let mut f = Frame::new(i, o);
    let c = p.call(slot::ON_PIECE, &mut f);
    let written = units[..f.out.units_written as usize]
        .iter()
        .map(|u| (u.class, u.source, u.amount))
        .collect();
    (c.outcome, written)
}

/// This crate's built cdylib, beside the test binary.
fn cdylib() -> std::path::PathBuf {
    let exe = std::env::current_exe().expect("the test binary has a path");
    let deps = exe.parent().expect("target/<profile>/deps").to_path_buf();
    let profile = deps.parent().expect("target/<profile>").to_path_buf();
    let file = busbar_plugin_loader::plugin_library_filename("busbar_plane_decisions_plugin");
    [profile.join(&file), deps.join(&file)]
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| panic!("this crate's cdylib ({file}) is not built"))
}

/// The door, loaded linked or dropped in, its host log `sink`.
fn load(d: &Dispatcher, dropped_in: bool, sink: Arc<HostLog>) -> Plugin<Plane> {
    let bound = Bind {
        instance: Arc::from(if dropped_in { "dropped" } else { "linked" }),
        max_inflight_cap: 8,
        sink,
        dispatcher: d.adopter(),
        conns: busbar_plugin_loader::dispatch::ConnTable::Probe,
    };
    if dropped_in {
        let stated = rendering_of(busbar_plane_decisions::door).expect("the door renders");
        load_dropped(&cdylib(), &stated, bound).expect("the dropped door loads")
    } else {
        let row = LinkedRow::of(busbar_plane_decisions::door).expect("the door states itself");
        load_linked(&row, bound).expect("the linked door loads")
    }
}

/// An `open` over settings the grammar refuses is REFUSED, as `validate` and `refresh` refuse
/// them, and the host's operator text for it (`plugin '<name>' open failed: <words>`) is the same
/// linked and dropped in.
#[test]
fn an_open_the_grammar_refuses_reads_the_same_both_ways() {
    let d = Dispatcher::new(DispatchConfig::default());
    let words: Vec<String> = [false, true]
        .into_iter()
        .map(|dropped_in| {
            let p = load(&d, dropped_in, Arc::new(HostLog::default()));
            let mut i: PlaneOpenIn = z();
            i.open.head = in_head();
            (i.open.generation, i.open.settings) = (1, octets(br#"{"modles":{}}"#));
            let mut o: PlaneOpenOut = z();
            o.open.head = out_head();
            assert_eq!(
                p.open(&mut Frame::new(i, o)).0.outcome,
                Outcome::Refused,
                "dropped-in {dropped_in}"
            );
            // A fresh load: the host's own open path, over the same settings.
            let p = load(&d, dropped_in, Arc::new(HostLog::default()));
            busbar_plugin_loader::dispatch::kinds::plane::open_door(
                &p,
                br#"{"modles":{}}"#,
                b"",
                None,
            )
            .expect_err("the open is refused")
        })
        .collect();
    assert_eq!(words[0], words[1]);
    assert!(
        words[0].starts_with("plugin '")
            && words[0].ends_with(
                "' open failed: unknown field `modles`, expected one of `models`, `hooks`, \
                 `upstream_credentials`"
            ),
        "{}",
        words[0]
    );
}

#[test]
fn an_unreadable_usage_line_reaches_the_host_log_dropped_in_as_linked() {
    let d = Dispatcher::new(DispatchConfig::default());
    let providers = [ProviderFacts {
        name: text(b"typesafe"),
        protocol: text(b"jev"),
        error_map: Blob::ABSENT,
    }];
    for dropped_in in [false, true] {
        let log = Arc::new(HostLog::default());
        let p = load(&d, dropped_in, log.clone());
        let mut i: PlaneOpenIn = z();
        i.open.head = in_head();
        (i.open.generation, i.open.settings) = (1, octets(SETTINGS));
        (i.providers, i.providers_len) = (providers.as_ptr(), providers.len());
        let mut o: PlaneOpenOut = z();
        o.open.head = out_head();
        assert_eq!(p.open(&mut Frame::new(i, o)).0.outcome, Outcome::Ready);

        let mut a: Frame<ArriveIn, ArriveOut> = Frame::new(z(), z());
        (a.input.head, a.out.head) = (in_head(), out_head());
        (a.input.unit, a.input.method, a.input.target) = (1, text(b"POST"), text(b"/v1/systemone"));
        a.input.body = octets(BODY);
        let mut estimates = [z::<UnitCount>(); 4];
        (a.input.units_buf, a.input.units_cap) = (estimates.as_mut_ptr(), estimates.len());
        assert_eq!(p.call(slot::ARRIVE, &mut a).outcome, Outcome::Ready);

        piece(&p, 1, FROM_KERNEL, 0, 0, b"", 1);
        piece(&p, 1, FROM_CALLER, PIECE_LAST, 0, BODY, 0);
        let (outcome, units) = piece(
            &p,
            1,
            FROM_FAR_END,
            PIECE_HAS_STATUS | PIECE_LAST,
            200,
            UNREADABLE,
            0,
        );
        assert_eq!(outcome, Outcome::Ready, "dropped-in {dropped_in}");
        assert!(
            units.contains(&(0, UNITS_FLOOR, 1)),
            "dropped-in {dropped_in}: the floor is billed: {units:?}"
        );
        let lines = log.0.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("usage is unreadable"))
                .count(),
            1,
            "dropped-in {dropped_in}: one line reaches the host log: {lines:?}"
        );
    }
}
