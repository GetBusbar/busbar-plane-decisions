//! This plane is the same plane everywhere it is compiled.
//!
//! Mirrors `busbar-plane-a2a`/`busbar-plane-mcp`'s own `invariance.rs`. The manifest names exactly
//! one busbar crate (the contract, which holds `ModelCfg` and `UpstreamCreds`) plus the serializer.
//! The forbidden list is `busbar-core`, `busbar-caps`, `busbar-kernel`, `busbar-unit-*`, the
//! sibling PLANE crates and every other plugin kind's crates ([`SIBLINGS`]: naming another plugin
//! would be a plane reading another plugin's private surface, never legitimate for any plane). `the_manifest_names_only_what_this_plane_may_name`
//! must never be relaxed to keep it green; if `busbar-kernel` returns to the manifest, that is the
//! line that is supposed to fail.

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn manifest() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("the manifest is readable")
}

/// The crates this plane may never name, by name or by prefix: every sibling plane (a plane names
/// only itself) and every crate of the other plugin kinds. A fixed list, not a directory listing:
/// this repository holds no sibling, so a list read off the tree would be empty and the gate could
/// never fail.
const SIBLINGS: &[&str] = &[
    "busbar-plane-llm",
    "busbar-plane-mcp",
    "busbar-plane-a2a",
    "busbar-plane-streaming",
    "busbar-transport-",
    "busbar-auth-",
    "busbar-secret-",
    "busbar-store-",
    "busbar-export-",
    "busbar-hook-",
];

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    for entry in std::fs::read_dir(dir)
        .expect("the source directory is readable")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            f(&path, &text);
        }
    }
}

/// The manifest declares no features at all.
#[test]
fn the_crate_declares_no_features() {
    let manifest = manifest();
    assert!(
        !manifest.contains("[features]"),
        "the plane declares cargo features, so it is not one plane"
    );
    assert!(
        !manifest.contains("optional = true"),
        "an optional dependency is a feature by another name"
    );
}

/// No item on the surface is behind a conditional-compilation attribute (test modules exempted).
#[test]
fn no_item_is_conditionally_compiled() {
    let mut offenders = Vec::new();
    walk(&src_dir(), &mut |path, text| {
        for (n, line) in text.lines().enumerate() {
            let line = line.trim_start();
            if line.starts_with("#[cfg(test)]") {
                continue;
            }
            if line.starts_with("#[cfg(") || line.starts_with("#![cfg(") {
                offenders.push(format!("{}:{}", path.display(), n + 1));
            }
        }
    });
    assert!(
        offenders.is_empty(),
        "conditionally compiled items: {offenders:?}"
    );
}

/// The crate carries the two lint gates the design requires of a plugin.
#[test]
fn the_crate_forbids_unsafe_and_undocumented_items() {
    let lib = std::fs::read_to_string(src_dir().join("lib.rs")).expect("the root is readable");
    assert!(lib.contains("#![forbid(unsafe_code)]"));
    assert!(lib.contains("#![deny(missing_docs)]"));
}

/// No source file contains the word the lint gate forbids.
#[test]
fn the_source_contains_no_unsafe_block() {
    let mut offenders = Vec::new();
    walk(&src_dir(), &mut |path, text| {
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if line.contains("unsafe ") {
                offenders.push(format!("{}:{}", path.display(), n + 1));
            }
        }
    });
    assert!(
        offenders.is_empty(),
        "unsafe code on the surface: {offenders:?}"
    );
}

/// The manifest names only the contract (which holds both explicitly-ruled model-serving config
/// types, `ModelCfg` and `UpstreamCreds`) and the serializer — never a genuinely kernel-side crate,
/// never the legacy plugin-facing one, never a sibling plane.
#[test]
fn the_manifest_names_only_what_this_plane_may_name() {
    let offenders = manifest_offenders(&manifest());
    assert!(
        offenders.is_empty(),
        "the plane depends on {offenders:?}, kernel-side or a sibling plugin"
    );
}

/// Every dependency `manifest` names that this plane may not name.
fn manifest_offenders(manifest: &str) -> Vec<String> {
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("the manifest has a dependency section");
    let mut forbidden: Vec<String> = vec![
        // The edge this plane alone had, and the one that reached banned source. It is a ban,
        // not an exception, since `UpstreamCreds` moved to `busbar-contract`.
        "busbar-api".to_string(),
        "busbar-core".to_string(),
        "busbar-caps".to_string(),
        "busbar-kernel".to_string(),
        "busbar-unit-".to_string(),
        "busbar-admin".to_string(),
    ];
    forbidden.extend(SIBLINGS.iter().map(|s| (*s).to_string()));
    let mut offenders = Vec::new();
    for line in deps.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let name = line.split_whitespace().next().unwrap_or_default();
        if forbidden.iter().any(|f| name.starts_with(f.as_str())) {
            offenders.push(name.to_string());
        }
    }
    offenders
}

/// RED (audit plane-decisions round cb0ed01e04, D33): the manifest gate can fail: a sibling plane
/// or another plugin named as a dependency is caught. RED before: the sibling list was read off a
/// directory that holds no plane crate, so it was empty and the gate could not fail.
#[test]
fn the_manifest_gate_catches_a_sibling_plugin() {
    for sibling in [
        "busbar-plane-llm",
        "busbar-plane-mcp",
        "busbar-plane-a2a",
        "busbar-plane-streaming",
        "busbar-transport-http",
        "busbar-auth-header",
        "busbar-secret-env",
        "busbar-store-memory",
        "busbar-export-otlp",
        "busbar-hook-ranking",
    ] {
        let manifest = format!("{}\n{sibling} = \"*\"\n", manifest());
        assert_eq!(
            manifest_offenders(&manifest),
            vec![sibling.to_string()],
            "{sibling} is not caught"
        );
    }
}

/// The comments cite the design in words, not in section numbers or binding identifiers.
#[test]
fn the_comments_cite_the_design_in_words() {
    let mut offenders = Vec::new();
    let mut check = |path: &Path, text: &str| {
        for (n, line) in text.lines().enumerate() {
            if line.contains('\u{a7}') {
                offenders.push(format!("{}:{}: section sign", path.display(), n + 1));
            }
        }
    };
    walk(&src_dir(), &mut check);
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"),
        &mut check,
    );
    assert!(
        offenders.is_empty(),
        "the source cites the design by section-sign rather than in words: {offenders:?}"
    );
}
