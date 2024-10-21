// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::upgrade_compatibility::compare_packages;
use insta::assert_snapshot;
use move_binary_format::CompiledModule;
use std::path::PathBuf;
use sui_move_build::BuildConfig;

#[test]
fn test_all_fail() {
    let (pkg_v1, pkg_v2) = get_packages("all");

    let result = compare_packages(pkg_v1, pkg_v2);
    assert!(result.is_err());
    let err = result.unwrap_err();

    assert_snapshot!(err.to_string());
}

#[test]
fn test_struct_missing() {
    let (pkg_v1, pkg_v2) = get_packages("struct_missing");
    let result = compare_packages(pkg_v1, pkg_v2);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(err.to_string());
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

fn get_packages(name: &str) -> (Vec<CompiledModule>, Vec<CompiledModule>) {
    let mut path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/fixtures/upgrade_errors/");
    path.push(format!("{}_v1", name));

    let pkg_v1 = BuildConfig::new_for_testing()
        .build(&path)
        .unwrap()
        .into_modules();

    let mut path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/fixtures/upgrade_errors/");
    path.push(format!("{}_v2", name));

    let pkg_v2 = BuildConfig::new_for_testing()
        .build(&path)
        .unwrap()
        .into_modules();

    (pkg_v1, pkg_v2)
}
