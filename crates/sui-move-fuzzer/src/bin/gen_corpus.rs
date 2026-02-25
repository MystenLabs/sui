// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generates a seed corpus for the Sui Move VM fuzzer.
//!
//! Usage:
//!   cargo run --bin gen_corpus
//!
//! Creates corpus directories with seed files for each fuzz target.

use std::fs;
use std::path::Path;

use sui_move_fuzzer::module_spec::{ModuleSpec, ModuleSpecBuilder};
use sui_move_fuzzer::seed_corpus;

fn write_seeds(dir: &Path, seeds: &[Vec<u8>]) {
    fs::create_dir_all(dir).expect("create corpus dir");
    for (i, seed) in seeds.iter().enumerate() {
        let path = dir.join(format!("seed_{i:04}.bin"));
        fs::write(&path, seed).expect("write seed");
    }
    println!("  wrote {} seeds to {}", seeds.len(), dir.display());
}

fn main() {
    let base = Path::new("corpus");

    // Raw module bytes for deser_roundtrip, e2e_publish_crash, ref_safety_diff
    println!("Generating raw module seeds...");
    let raw_seeds = seed_corpus::generate_seeds(100);
    write_seeds(&base.join("deser_roundtrip"), &raw_seeds);
    write_seeds(&base.join("e2e_publish_crash"), &raw_seeds);
    write_seeds(&base.join("ref_safety_diff"), &raw_seeds);

    // Structured seeds for grammar-based targets
    println!("Generating structured seeds...");
    let structured_seeds = seed_corpus::generate_structured_seeds(100);
    write_seeds(&base.join("verifier_crash"), &structured_seeds);
    write_seeds(&base.join("bounds_escape"), &structured_seeds);
    write_seeds(&base.join("e2e_publish_execute"), &structured_seeds);
    write_seeds(&base.join("ref_safety_diff_gen"), &structured_seeds);
    write_seeds(&base.join("verifier_soundness"), &structured_seeds);

    // Load hand-written ModuleSpec JSON files from specs/ directory.
    // These are curated seeds that target specific verifier paths.
    let specs_dir = Path::new("specs");
    if specs_dir.exists() {
        println!("Loading ModuleSpec seeds from specs/...");
        let mut spec_seeds: Vec<Vec<u8>> = Vec::new();

        let mut entries: Vec<_> = fs::read_dir(specs_dir)
            .expect("read specs dir")
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let json = match fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("  skipping {}: {e}", path.display());
                    continue;
                }
            };
            let spec: ModuleSpec = match serde_json::from_str(&json) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("  invalid JSON in {}: {e}", path.display());
                    continue;
                }
            };
            match ModuleSpecBuilder::build(&spec) {
                Ok(module) => {
                    let mut bytes = Vec::new();
                    if module.serialize(&mut bytes).is_ok() {
                        println!("  built seed from {}", path.display());
                        spec_seeds.push(bytes);
                    } else {
                        eprintln!("  serialization failed for {}", path.display());
                    }
                }
                Err(e) => {
                    eprintln!("  build error for {}: {e}", path.display());
                }
            }
        }

        if !spec_seeds.is_empty() {
            // Deposit spec-based seeds into all grammar-based targets.
            for target in &[
                "verifier_crash",
                "ref_safety_diff_gen",
                "verifier_soundness",
            ] {
                let dir = base.join(target);
                fs::create_dir_all(&dir).expect("create corpus dir");
                for (i, seed) in spec_seeds.iter().enumerate() {
                    let filename = dir.join(format!("spec_{:04}.bin", i));
                    fs::write(&filename, seed).expect("write spec seed");
                }
                println!(
                    "  wrote {} spec seeds to {}",
                    spec_seeds.len(),
                    dir.display()
                );
            }
        }
    }

    println!("Done! Seed corpus written to {}", base.display());
}
