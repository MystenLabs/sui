// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::upgrade_compatibility::{compare_packages, missing_module_diag};
use insta::assert_snapshot;
use move_binary_format::CompiledModule;
use move_command_line_common::files::FileHash;
use move_compiler::diagnostics::report_diagnostics_to_buffer;
use move_compiler::shared::files::{FileName, FilesSourceText};
use move_core_types::identifier::Identifier;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use sui_move_build::BuildConfig;
use sui_move_build::CompiledPackage;
use sui_types::move_package::UpgradePolicy;

#[test]
fn test_all() {
    let (mods_v1, pkg_v2, path) = get_packages("all");
    let result = compare_packages(mods_v1, pkg_v2, path, UpgradePolicy::Compatible);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_declarations_missing() {
    let (pkg_v1, pkg_v2, path) = get_packages("declaration_errors");
    let result = compare_packages(pkg_v1, pkg_v2, path, UpgradePolicy::Compatible);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_function() {
    let (pkg_v1, pkg_v2, path) = get_packages("function_errors");
    let result = compare_packages(pkg_v1, pkg_v2, path, UpgradePolicy::Compatible);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_struct() {
    let (pkg_v1, pkg_v2, path) = get_packages("struct_errors");
    let result = compare_packages(pkg_v1, pkg_v2, path, UpgradePolicy::Compatible);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_enum() {
    let (pkg_v1, pkg_v2, path) = get_packages("enum_errors");
    let result = compare_packages(pkg_v1, pkg_v2, path, UpgradePolicy::Compatible);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_type_param() {
    let (pkg_v1, pkg_v2, path) = get_packages("type_param_errors");
    let result = compare_packages(pkg_v1, pkg_v2, path, UpgradePolicy::Compatible);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_friend_link_ok() {
    let (pkg_v1, pkg_v2, path) = get_packages("friend_linking");
    // upgrade compatibility ignores friend linking
    assert!(compare_packages(pkg_v1, pkg_v2, path, UpgradePolicy::Compatible).is_ok());
}

#[test]
fn test_entry_linking_ok() {
    let (pkg_v1, pkg_v2, path) = get_packages("entry_linking");
    // upgrade compatibility ignores entry linking
    assert!(compare_packages(pkg_v1, pkg_v2, path, UpgradePolicy::Compatible).is_ok());
}

#[test]
fn test_malformed_toml() {
    /// note: the first examples empty and whitespace shouldn't occur in practice
    /// since a Move.toml which is empty will not build
    for malformed_pkg in [
        "empty",
        "whitespace",
        "addresses_first",
        "starts_second_line",
    ] {
        let move_pkg_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/unit_tests/fixtures/upgrade_errors/malformed_move_toml/")
            .join(malformed_pkg);
        let result =
            missing_module_diag(&Identifier::from_str("identifier").unwrap(), &move_pkg_path);

        let move_toml: Arc<str> = fs::read_to_string(move_pkg_path.join("Move.toml"))
            .unwrap()
            .into();
        let file_hash = FileHash::new(&move_toml);
        let mut files = FilesSourceText::new();
        let filename = FileName::from(move_pkg_path.join("Move.toml").to_string_lossy());
        files.insert(file_hash, (filename, move_toml));

        let output = String::from_utf8(report_diagnostics_to_buffer(
            &files.into(),
            result.unwrap(),
            false,
        ))
        .unwrap();
        assert_snapshot!(malformed_pkg, output);
    }
}

fn get_packages(name: &str) -> (Vec<CompiledModule>, CompiledPackage, PathBuf) {
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

    (mods_v1, pkg_v2, path)
}

/// Snapshots will differ on each machine, normalize to prevent test failures
fn normalize_path(err_string: String) -> String {
    //test
    let re = regex::Regex::new(r"^(.*)┌─ .*(\/fixtures\/.*\.move:\d+:\d+)$").unwrap();
    err_string
        .lines()
        .map(|line| re.replace(line, "$1┌─ $2").into_owned())
        .collect::<Vec<String>>()
        .join("\n")
}
