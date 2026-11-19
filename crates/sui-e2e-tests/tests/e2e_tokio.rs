// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that run without the simulator. The e2e_sim_* binaries hold the simulator tests; they
// compile to nothing outside msim, except under clippy so that it still lints them.
#![cfg(not(msim))]

mod db_shell_tests;
mod protocol_version_tests;
mod traffic_control_tests;

use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

/// Source markers of tests that run only outside msim.
const NON_SIM_MARKERS: [&str; 3] = ["#[tokio::test", "cfg_attr(msim, ignore)", "cfg(not(msim))"];

/// Returns the Rust source of a test file, or of every .rs file under a directory test. Symlinks
/// are not followed.
fn test_sources(path: &Path) -> String {
    let file_type = std::fs::symlink_metadata(path).unwrap().file_type();
    if file_type.is_dir() {
        let mut sources = String::new();
        for entry in std::fs::read_dir(path).unwrap() {
            sources += &test_sources(&entry.unwrap().path());
        }
        sources
    } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
        std::fs::read_to_string(path).unwrap()
    } else {
        String::new()
    }
}

/// Returns the modules a test binary root declares. Each must be a plain `mod name;` line, so that
/// a cfg attribute cannot disable a module that the check counts as declared.
fn declared_modules(root: &Path) -> BTreeSet<String> {
    let source = std::fs::read_to_string(root).unwrap();
    for line in source.lines().map(str::trim) {
        assert!(
            !line.starts_with("#[cfg"),
            "{} has a cfg attribute on a module: {line}",
            root.display()
        );
    }
    source
        .lines()
        .filter_map(|line| line.trim().strip_prefix("mod ")?.strip_suffix(';'))
        .map(str::to_owned)
        .collect()
}

/// Cargo.toml sets `autotests = false`, so a file in tests/ that no binary declares as a module is
/// never compiled, and cargo does not warn. A file with simulator tests must be in an e2e_sim_*
/// binary that Cargo.toml lists. A file with tests that run only outside msim must be in this one.
#[test]
fn every_test_file_is_in_a_test_binary() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest_lines: BTreeSet<String> = std::fs::read_to_string(manifest_dir.join("Cargo.toml"))
        .unwrap()
        .lines()
        .map(|line| line.trim().to_owned())
        .collect();
    let tests_dir = manifest_dir.join("tests");
    let tokio_modules = declared_modules(&tests_dir.join("e2e_tokio.rs"));
    let mut sim_modules = BTreeSet::new();
    let mut test_files: Vec<(String, PathBuf)> = Vec::new();

    for entry in std::fs::read_dir(&tests_dir).unwrap() {
        let path = entry.unwrap().path();
        let file_name = path.file_name().unwrap().to_str().unwrap().to_owned();
        // Cargo's test autodiscovery covers tests/<name>.rs and tests/<name>/main.rs, and skips
        // hidden files such as editor lock files.
        if file_name.starts_with('.') {
            continue;
        }
        let name = if path.is_dir() && path.join("main.rs").exists() {
            file_name
        } else if let Some(stem) = file_name.strip_suffix(".rs") {
            stem.to_owned()
        } else {
            continue;
        };
        if name.starts_with("e2e_sim_") {
            assert!(
                manifest_lines.contains(&format!("path = \"tests/{name}.rs\"")),
                "tests/{name}.rs has no [[test]] entry in Cargo.toml"
            );
            sim_modules.extend(declared_modules(&path));
        } else if name != "e2e_tokio" {
            test_files.push((name, path));
        }
    }

    for (name, path) in test_files {
        let sources = test_sources(&path);
        let in_sim = sim_modules.contains(&name);
        let in_tokio = tokio_modules.contains(&name);
        assert!(
            in_sim || in_tokio,
            "tests/{name} is not declared in any test binary"
        );
        if sources.contains("#[sim_test") {
            assert!(
                in_sim,
                "tests/{name} has #[sim_test]s but is not declared in any tests/e2e_sim_*.rs"
            );
        }
        if NON_SIM_MARKERS
            .iter()
            .any(|marker| sources.contains(marker))
        {
            assert!(
                in_tokio,
                "tests/{name} has tests that run only outside msim but is not declared in \
                 tests/e2e_tokio.rs"
            );
        }
    }
}
