// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! WHAT RELAYING A FAR-END ANSWER ALLOCATES (THE DESIGN section 11.2: "no allocation and no
//! blocking on the hot path"): a piece of the far end's answer is read as it passes and written
//! straight into the host's reply buffer, so relaying piece after piece of an answer allocates
//! nothing, however long the answer runs.

#![allow(unsafe_code)]

mod door_harness;

use busbar_contract::abi::mechanism::call::Outcome;
use busbar_test_kit::alloc_counter::{allocations_of, last_seen, watch, Counting};
use door_harness::{open, Bufs, Given};

/// The test binary's allocator: the test kit's counting allocator (a dev-dependency), the one
/// every allocation gate of the fleet reads.
#[global_allocator]
static A: Counting = Counting::new();

/// RED (audit plane-decisions#8): 256 pieces of a 1 MiB answer, each 4 KiB, relayed through the
/// door: no allocation (predev kept the whole answer, growing a buffer, and copied each piece
/// again before writing it).
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
    // Warm: one piece outside the window.
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

/// RED (audit plane-decisions round cb0ed01e04, D30): a body larger than the reply buffer is owed
/// across re-calls, and once the last byte of it is paid the unit lets the debt's buffer go, while
/// it still waits on its far end. The freed-key witness sees the buffer freed by the marker it
/// carried. RED before: the unit kept a body-sized dead buffer until its far end's last piece.
#[test]
fn a_paid_debt_is_freed_before_the_far_end_answers() {
    const MARK: &[u8] = b"OWED-DEBT-MARKER-6c1f0a9e";
    watch(1, MARK);
    let door = open(r#"{"models":{"jev":{"provider":"typesafe"}}}"#);
    let mut body = br#"{"state":{"blob":""#.to_vec();
    body.resize(body.len() + (1 << 20), b'x');
    body.extend_from_slice(MARK);
    body.extend_from_slice(br#""}}"#);
    assert_eq!(door.arrive(1, &body).outcome, Outcome::Ready);
    let mut bufs = Bufs::new(4096);
    door.piece(1, Given::attempt(1), &mut bufs);
    assert_eq!(last_seen(1), None, "nothing owed yet");
    let sent = door.piece(1, Given::caller(&body), &mut bufs);
    assert!(sent.far, "{:?}", sent.outcome);
    let freed = last_seen(1).map_or(0, |seen| seen.block);
    assert!(
        freed >= 1 << 19,
        "the paid debt's buffer is still held while the far end is awaited (freed: {freed})"
    );
    let last = door.piece(1, Given::whole(200, br#"{"usage":{"units":1}}"#), &mut bufs);
    assert!(last.done, "{last:?}");
}

/// The far end's answer relayed through a reply buffer smaller than its pieces: each piece is owed
/// across re-calls, and the answer side's debt buffer is reused piece after piece, so relaying
/// still allocates nothing (only a debt bound for the far end is let go once paid).
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
    // Warm: one piece outside the window.
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
