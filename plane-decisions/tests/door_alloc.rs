// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! Allocation behavior when relaying an upstream answer.

#![allow(unsafe_code)]

mod door_harness;

use busbar_contract::abi::mechanism::call::Outcome;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Default, Debug)]
struct Debt {
    size: usize,
    address: usize,
    retired: bool,
    allocations: usize,
    frees: usize,
    reallocations: usize,
}

thread_local! {
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    static DEBT: Cell<Option<Debt>> = const { Cell::new(None) };
}

struct Counting;

fn counted() {
    let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
}

fn allocated(ptr: *mut u8, size: usize) {
    let _ = DEBT.try_with(|cell| {
        if let Some(mut debt) = cell.get() {
            if !ptr.is_null() && size == debt.size {
                debt.allocations += 1;
                if debt.allocations == 1 {
                    debt.address = ptr as usize;
                }
                cell.set(Some(debt));
            }
        }
    });
}

// SAFETY: memory operations forward the caller's arguments to System. Observations use
// const thread-local metadata only; they never dereference or expose an allocation address.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        counted();
        let ptr = unsafe { System.alloc(layout) };
        allocated(ptr, layout.size());
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        counted();
        let ptr = unsafe { System.alloc_zeroed(layout) };
        allocated(ptr, layout.size());
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = DEBT.try_with(|cell| {
            if let Some(mut debt) = cell.get() {
                if debt.address != 0 && debt.address == ptr as usize && !debt.retired {
                    debt.retired = true;
                    debt.frees += 1;
                    cell.set(Some(debt));
                }
            }
        });
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        counted();
        let next = unsafe { System.realloc(ptr, layout, size) };
        let _ = DEBT.try_with(|cell| {
            if let Some(mut debt) = cell.get() {
                let tracked = debt.address != 0 && debt.address == ptr as usize && !debt.retired;
                if tracked || layout.size() == debt.size || size == debt.size {
                    debt.reallocations += 1;
                }
                if tracked && !next.is_null() {
                    debt.address = next as usize;
                }
                cell.set(Some(debt));
            }
        });
        next
    }
}

fn allocations_of(f: impl FnOnce()) -> u64 {
    let before = ALLOCATIONS.with(Cell::get);
    f();
    ALLOCATIONS.with(Cell::get) - before
}
use door_harness::{open, Bufs, Given};

#[global_allocator]
static A: Counting = Counting;

#[test]
fn relaying_an_answer_piece_by_piece_allocates_nothing() {
    let door = open(r#"{"models":{"jev":{"provider":"typesafe"}}}"#);
    let mut bufs = Bufs::new(1 << 16);
    assert_eq!(door.arrive(1, br#"{"state":{}}"#).outcome, Outcome::Ready);
    door.piece(1, Given::attempt(1), &mut bufs);
    door.piece(1, Given::caller(br#"{"state":{}}"#), &mut bufs);
    let head = door.piece(
        1,
        Given::far(Some(200), false, br#"{"answers":{"blob":""#),
        &mut bufs,
    );
    assert_eq!(head.outcome, Some(Outcome::Ready));
    let filler = [b'x'; 4096];
    let piece = Given::far(None, false, &filler);

    door.piece(1, piece, &mut bufs);
    let mut relayed = 0;
    let count = allocations_of(|| {
        for _ in 0..256 {
            relayed += door.piece_bytes(1, piece, &mut bufs);
        }
    });
    assert_eq!(relayed, 256 * filler.len(), "every byte is relayed");
    assert_eq!(count, 0, "relaying 256 pieces allocated {count} times");
    let last = door.piece(
        1,
        Given::far(None, true, br#""},"usage":{"units":4}}"#),
        &mut bufs,
    );
    assert_eq!(last.units.first().map(|u| u.2), Some(4), "{last:?}");
}

#[test]
fn a_paid_debt_is_freed_before_the_far_end_answers() {
    const MARK: &[u8] = b"OWED-DEBT-MARKER-6c1f0a9e";
    const REPLY_CAP: usize = 4096;
    let door = open(r#"{"models":{"jev":{"provider":"typesafe"}}}"#);
    let mut body = br#"{"state":{"blob":""#.to_vec();
    body.resize(body.len() + (1 << 20), b'x');
    body.extend_from_slice(MARK);
    body.extend_from_slice(br#""}}"#);
    assert_eq!(door.arrive(1, &body).outcome, Outcome::Ready);
    let mut bufs = Bufs::new(REPLY_CAP);
    door.piece(1, Given::attempt(1), &mut bufs);

    let tail_len = body.len() - REPLY_CAP;
    assert!(tail_len >= 1 << 19, "this is a body-sized debt");
    DEBT.with(|debt| {
        assert!(debt.get().is_none());
        debt.set(Some(Debt {
            size: tail_len,
            ..Debt::default()
        }));
    });
    let sent = door.piece_owed_to_far(1, Given::caller(&body), &mut bufs);
    let observed = DEBT.with(|debt| debt.take().expect("the debt observation is active"));
    assert_eq!(sent, body.len(), "every caller byte went upstream");
    assert_eq!(
        (observed.allocations, observed.frees, observed.reallocations),
        (1, 1, 0),
        "the unique debt allocation must be freed before the far end answers: {observed:?}"
    );
    let last = door.piece(1, Given::whole(200, br#"{"usage":{"units":1}}"#), &mut bufs);
    assert!(last.done, "{last:?}");
}

/// The far end's answer relayed through a reply buffer smaller than its pieces: each piece is owed
#[test]
fn relaying_pieces_larger_than_the_reply_buffer_allocates_nothing() {
    let door = open(r#"{"models":{"jev":{"provider":"typesafe"}}}"#);
    let mut bufs = Bufs::new(1024);
    assert_eq!(door.arrive(1, br#"{"state":{}}"#).outcome, Outcome::Ready);
    door.piece(1, Given::attempt(1), &mut bufs);
    door.piece(1, Given::caller(br#"{"state":{}}"#), &mut bufs);
    door.piece(
        1,
        Given::far(Some(200), false, br#"{"answers":{"blob":""#),
        &mut bufs,
    );
    let filler = [b'x'; 4096];
    let piece = Given::far(None, false, &filler);

    door.piece(1, piece, &mut bufs);
    let mut relayed = 0;
    let count = allocations_of(|| {
        for _ in 0..64 {
            relayed += door.piece_owed(1, piece, &mut bufs);
        }
    });
    assert_eq!(relayed, 64 * filler.len(), "every byte is relayed");
    assert_eq!(count, 0, "relaying 64 owed pieces allocated {count} times");
}
