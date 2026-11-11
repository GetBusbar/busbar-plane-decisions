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
/// the per-repo `release.yml`'s `declares_file:`).
const SIGNED: &str = "plane-decisions/declares.json";

/// The release signs the file the Statement carries (`SIGNED`, the one named constant; the first test
/// pins the Statement to that file), and no other: the recipe's entry must name `SIGNED`.
#[test]
fn the_release_signs_the_statements_declares_file() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/..");
    // Read the explicit recipe, the admitted host checkout, or a local sibling checkout.
    // Its entry for this repo must name the same file.
    let named_path = std::env::var("BUSBAR_PLUGINS_YAML").ok();
    let path = named_path
        .clone()
        .or_else(|| {
            std::env::var("BUSBAR_CHECKOUT")
                .ok()
                .map(|host| format!("{host}/plugins.yaml"))
        })
        .unwrap_or_else(|| format!("{root}/../busbar/plugins.yaml"));
    let read = std::fs::read_to_string(&path);
    // A recipe named outright, or any CI run (which supplies the admitted host checkout), must be
    // read: only a local run without the busbar tree may pass without judging it, and says so.
    let required = named_path.is_some() || std::env::var_os("CI").is_some();
    match &read {
        Err(e) if required => panic!("the packaging recipe {path} cannot be read: {e}"),
        Err(_) => eprintln!(
            "the packaging recipe {path} is not at hand: the release's declares file is not judged"
        ),
        Ok(_) => {}
    }
    if let Ok(yaml) = read {
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
