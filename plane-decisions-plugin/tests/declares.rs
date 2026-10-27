// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! THE DECLARES RIDE THE STATEMENT (ARCHITECT ruling 2026-10-21; BUSBAR-1.6.0.md l.246): the door's
//! Statement, read back as the loader reads a linked row, states this plugin's whole `declares`
//! document under `EXT_DECLARES`, byte for byte the `declares.json` the release signs.

/// The one declares file: the one the door's Statement carries and the one `release.yml` signs.
const DECLARES: &[u8] = include_bytes!("../../plane-decisions/declares.json");

#[test]
fn the_statement_states_the_whole_declares_document() {
    let row = busbar_plugin_loader::dispatch::LinkedRow::of(busbar_plane_decisions::door).unwrap();
    let read = busbar_contract::abi::mechanism::rendering::read(&row.statement).unwrap();
    assert_eq!(
        busbar_contract::abi::mechanism::extensions::declares(&read.extensions.bytes),
        Some(DECLARES),
    );
}

/// The release signs the file the Statement carries, and no other: a second declares file would be
/// a dropped-in declaration that disagrees with the linked door's.
#[test]
fn the_release_signs_the_statements_declares_file() {
    let release = include_str!("../../.github/workflows/release.yml");
    assert!(
        release.contains("declares_file: \"plane-decisions/declares.json\""),
        "release.yml signs a declares file other than the one the door's Statement carries"
    );
}
