//! The plane is pure, and this is what says so.
//!
//! Mirrors `busbar-plane-a2a`'s own `purity.rs`, and no longer carries the ONE difference it used
//! to. The legacy plugin-facing crate was exempted from the "names no kernel-side crate" scan here,
//! because this crate named it for `UpstreamCreds`; it is on the forbidden list now, like every
//! other non-contract busbar crate. The exemption had a price nobody was paying attention to: that
//! crate depends on `sha2`, and `sha2 -> cpufeatures -> libc` is banned source in a pure plugin
//! kind. `UpstreamCreds` moved to `busbar_contract::config` verbatim and the edge is gone, so the
//! exemption has nothing left to excuse. `ModelCfg` needed no exception either — it is
//! `busbar_contract::config::ModelCfg` (DECISIONS #40/#38), the crate's ordinary contract
//! dependency. `busbar_kernel` stays ON the forbidden list and this crate no longer names it
//! anywhere: `src/registry.rs` and its sibling `src/tests/registry.rs` were the only two lines of
//! source that did, and both files are gone — an unreachable generation-1 `PlaneDecl` constant that
//! nothing outside this crate ever read.
//!
//! These are the SOURCE scans: they read files, never the crate, so they stay an integration test.
//! The plane's type-level purity (its decode and encode steps, its clock independence and its key)
//! is held in-crate, in `src/tests/purity.rs`, since the crate's only public item is its door.
//! The crate's test modules (`src/tests`) are exempt from the scans: they are compiled only when
//! the tests are, never shipped, and hold the one allocation counter the gates read.

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    for entry in std::fs::read_dir(dir)
        .expect("the source directory is readable")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).expect("a source file is readable");
            f(&path, &text);
        }
    }
}

fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with('*') || t.starts_with("/*")
}

/// Every OTHER plane crate's module spelling (`busbar_plane_<name>`), read live off the `crates/`
/// directory rather than spelled by name — the forbidden list below forbids by SHAPE
/// (`busbar-plane-*`, minus this crate itself) instead of a plane naming its siblings, which is the
/// broker-law violation DECISION #8 forbids in the first place. It also stays true automatically if
/// the plane roster (#48) ever changes.
fn sibling_plane_modules() -> Vec<String> {
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
        .map(|name| name.replace('-', "_"))
        .collect();
    out.sort();
    out
}

/// The plane keeps no state of its own between calls.
#[test]
fn the_plane_holds_no_interior_state() {
    let forbidden = [
        "Cell<",
        "RefCell",
        "UnsafeCell",
        "Mutex<",
        "RwLock",
        "AtomicUsize",
        "AtomicU64",
        "AtomicU32",
        "AtomicBool",
        "OnceLock",
        "OnceCell",
        "LazyLock",
        "lazy_static",
        "thread_local",
        "static mut",
    ];
    let mut offenders = Vec::new();
    walk(&src_dir(), &mut |path, text| {
        for (n, line) in text.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            for name in forbidden {
                if line.contains(name) {
                    offenders.push(format!("{}:{}: {name}", path.display(), n + 1));
                }
            }
        }
    });
    assert!(
        offenders.is_empty(),
        "the plane holds interior state: {offenders:?}"
    );
}

/// The plane performs no input and no output.
#[test]
fn the_plane_performs_no_input_or_output() {
    let forbidden = [
        "std::fs",
        "std::net",
        "std::process",
        "std::thread",
        "std::io",
        "SystemTime",
        "Instant::now",
        "tokio",
        "reqwest",
        "async fn",
        "await",
        "env!",
        "std::env",
        "println!",
        "eprintln!",
    ];
    let mut offenders = Vec::new();
    walk(&src_dir(), &mut |path, text| {
        for (n, line) in text.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            for name in forbidden {
                if line.contains(name) {
                    offenders.push(format!("{}:{}: {name}", path.display(), n + 1));
                }
            }
        }
    });
    assert!(
        offenders.is_empty(),
        "the plane reaches outside itself: {offenders:?}"
    );
}

/// The plane names no GENUINELY kernel-side crate.
///
/// The legacy plugin-facing crate IS on this list now (first entry). It was the deliberate
/// exception for as long as this crate named it for `UpstreamCreds`, and that exception was the
/// single edge by which a pure plane reached banned source (`sha2 -> cpufeatures -> libc`). The
/// type moved to `busbar_contract::config` and the entry below is what stops the edge coming back
/// by the same door. `busbar_kernel` has been on it throughout, and this test was RED on exactly
/// two source lines —
/// `src/registry.rs`'s `use` of `PlaneDecl` and `src/tests/registry.rs`'s `BuildCtx` literal —
/// until both files were deleted as unreachable code. It is GREEN now and is the source-side half
/// of this crate's #40 witness (the manifest-side half is `invariance.rs`'s
/// `the_manifest_names_only_what_this_plane_may_name`).
#[test]
fn the_plane_names_no_kernel_side_crate() {
    let mut offenders = Vec::new();
    walk(&src_dir(), &mut |path, text| {
        for (n, name) in named_offenders(text) {
            offenders.push(format!("{}:{}: {name}", path.display(), n + 1));
        }
    });
    assert!(
        offenders.is_empty(),
        "the plane names kernel-side crates: {offenders:?}"
    );
}

/// Every `(line index, crate)` of a kernel-side or sibling crate `text` names outside a comment.
fn named_offenders(text: &str) -> Vec<(usize, String)> {
    let mut forbidden: Vec<String> = vec![
        "busbar_api".to_string(),
        "busbar_caps".to_string(),
        "busbar_kernel".to_string(),
        "busbar_unit".to_string(),
        "busbar_core".to_string(),
        "busbar_admin".to_string(),
    ];
    forbidden.extend(sibling_plane_modules());
    let mut offenders = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if is_comment(line) {
            continue;
        }
        for name in &forbidden {
            if line.contains(name.as_str()) {
                offenders.push((n, name.clone()));
            }
        }
    }
    offenders
}

/// RED (audit plane-decisions round cb0ed01e04, D33): the source gate can fail: a line naming a
/// sibling plane or another plugin's crate is caught. RED before: the sibling list was read off a
/// directory that holds no plane crate, so it was empty and the gate could not fail.
#[test]
fn the_source_gate_catches_a_sibling_plugin() {
    for sibling in [
        "busbar_plane_llm",
        "busbar_plane_mcp",
        "busbar_plane_a2a",
        "busbar_plane_streaming",
        "busbar_transport_http",
        "busbar_auth_header",
        "busbar_secret_env",
        "busbar_store_memory",
        "busbar_export_otlp",
        "busbar_hook_ranking",
    ] {
        let text = format!("use {sibling}::Thing;\n");
        assert_eq!(named_offenders(&text).len(), 1, "{sibling} is not caught");
    }
}

/// The plane never hands back a decision, an amount or a credential.
#[test]
fn the_plane_names_no_money_and_no_decision() {
    let forbidden = [
        "nano_units",
        "nanounits",
        "unit_price",
        "price_micros",
        "charge(",
        "admit_decision",
        "allow(",
        "deny(",
        "credential_bytes",
        "secret_value",
        "credential_token",
    ];
    let mut offenders = Vec::new();
    walk(&src_dir(), &mut |path, text| {
        for (n, line) in text.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            let t = line.trim_start();
            if t.starts_with("#[") || t.starts_with("#![") {
                continue;
            }
            for name in forbidden {
                if line.contains(name) {
                    offenders.push(format!("{}:{}: {name}", path.display(), n + 1));
                }
            }
        }
    });
    assert!(
        offenders.is_empty(),
        "the plane names money or decisions: {offenders:?}"
    );
}
