//! This plane is the same plane everywhere it is compiled.
//!
//! Mirrors `busbar-plane-a2a`/`busbar-plane-mcp`'s own `invariance.rs`. The manifest names exactly
//! one busbar crate (the contract, which holds `ModelCfg` and `UpstreamCreds`) plus the serializer.
//! The forbidden list is `busbar-core`, `busbar-caps`, `busbar-kernel`, `busbar-unit-*`, and the
//! sibling PLANE crates (naming another plane would be a plane reading another plane's private
//! surface, never legitimate for any plane). `the_manifest_names_only_what_this_plane_may_name`
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

/// Every OTHER plane crate directory under `crates/`, read live off the tree rather than spelled by
/// name — the forbidden list below forbids by SHAPE (`busbar-plane-*`, minus this crate itself)
/// instead of a plane naming its siblings, which would be a broker-law violation. It also stays
/// true automatically if the plane roster ever changes.
fn sibling_plane_dirs() -> Vec<String> {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this crate sits under crates/")
        .to_path_buf();
    let this = Path::new(env!("CARGO_MANIFEST_DIR"))
        .file_name()
        .and_then(|n| n.to_str())
        .expect("this crate has a directory name")
        .to_string();
    let mut out: Vec<String> = std::fs::read_dir(&crates_dir)
        .expect("crates/ is readable")
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with("busbar-plane-") && *name != this)
        .collect();
    out.sort();
    out
}

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
    let manifest = manifest();
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("the manifest has a dependency section");
    for line in deps.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let name = line.split_whitespace().next().unwrap_or_default();
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
        forbidden.extend(sibling_plane_dirs());
        for forbidden in &forbidden {
            assert!(
                !name.starts_with(forbidden.as_str()),
                "the plane depends on {name}, which is kernel-side or a sibling plane"
            );
        }
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
