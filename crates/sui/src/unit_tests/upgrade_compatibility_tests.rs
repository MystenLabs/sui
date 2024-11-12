// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::upgrade_compatibility::compare_packages;
use insta::assert_snapshot;
use move_binary_format::CompiledModule;
use std::path::PathBuf;
use sui_move_build::BuildConfig;
use sui_move_build::CompiledPackage;

#[test]
#[should_panic]
fn test_all_fail() {
    let (mods_v1, pkg_v2) = get_packages("all");

    // panics: Not all errors are implemented yet
    compare_packages(mods_v1, pkg_v2).unwrap();
}

#[test]
fn test_declarations_missing() {
    let (pkg_v1, pkg_v2) = get_packages("declarations_missing");
    let result = compare_packages(pkg_v1, pkg_v2);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_friend_link_ok() {
    let (pkg_v1, pkg_v2) = get_packages("friend_linking");
    // upgrade compatibility ignores friend linking
    assert!(compare_packages(pkg_v1, pkg_v2).is_ok());
}

#[test]
fn test_entry_linking_ok() {
    let (pkg_v1, pkg_v2) = get_packages("entry_linking");
    // upgrade compatibility ignores entry linking
    assert!(compare_packages(pkg_v1, pkg_v2).is_ok());
}

fn get_packages(name: &str) -> (Vec<CompiledModule>, CompiledPackage) {
    let mut path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/fixtures/upgrade_errors/");
    path.push(format!("{}_v1", name));

    let mods_v1 = BuildConfig::new_for_testing()
        .build(&path)
        .unwrap()
        .into_modules();

    let mut path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/fixtures/upgrade_errors/");
    path.push(format!("{}_v2", name));

    let pkg_v2 = BuildConfig::new_for_testing().build(&path).unwrap();

    (mods_v1, pkg_v2)
}

/// Snapshots will differ on each machine, normalize to prevent test failures
fn normalize_path(err_string: String) -> String {
    //test
    let re = regex::Regex::new(r"^  ┌─ .*(\/fixtures\/.*\.move:\d+:\d+)$").unwrap();
    err_string
        .lines()
        .map(|line| re.replace(line, "  ┌─ $1").into_owned())
        .collect::<Vec<String>>()
        .join("\n")
}
