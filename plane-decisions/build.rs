// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 Busbar Inc and contributors

//! The composition root reads this plane's root cells and real-binary black-box case directory
//! through the `CELLS` and `BLACKBOX` metadata of this crate's existing `links` row.

use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=build.rs");

    println!(
        "cargo::metadata=cells={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
            .join("tests/root/mod.rs")
            .display()
    );
    println!(
        "cargo::metadata=blackbox={}",
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
            .join("tests/blackbox")
            .display()
    );
    Ok(())
}
