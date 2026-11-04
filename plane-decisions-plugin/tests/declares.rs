// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! The door Statement carries the complete declarations document.

/// The one declares file: the one the door's Statement carries and the one the release signs.
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

/// The packaging recipe's `declares:` for this plugin (busbar's `plugins.yaml` entry, which replaced
const SIGNED: &str = "plane-decisions/declares.json";

/// The release signs the file the Statement carries (`SIGNED`, the one named constant; the first test
/// pins the Statement to that file), and no other: the recipe's entry must name `SIGNED`.
#[test]
fn the_release_signs_the_statements_declares_file() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    // The recipe itself, when the busbar tree is at hand (`BUSBAR_PLUGINS_YAML`, or a sibling
    // checkout): its entry for this repo names the same file.
    let yaml = std::env::var("BUSBAR_PLUGINS_YAML")
        .unwrap_or_else(|_| format!("{root}/../busbar/plugins.yaml"));
    if let Ok(yaml) = std::fs::read_to_string(yaml) {
        let entry = yaml
            .split("\n  - repo: ")
            .find(|e| {
                e.trim_start_matches("- repo: ")
                    .starts_with("busbar-plane-decisions\n")
            })
            .expect("plugins.yaml has an entry for this repo");
        let named = entry
            .lines()
            .find_map(|l| l.trim().strip_prefix("declares:"))
            .map(|v| v.trim().trim_matches('"'))
            .expect("the entry names its declares file");
        assert_eq!(
            named, SIGNED,
            "plugins.yaml signs a declares file other than the one the door's Statement carries"
        );
    }
}
