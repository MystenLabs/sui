// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use insta::assert_snapshot;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use crate::upgrade_compatibility::{compare_packages, missing_module_diag, FormattedField};

use move_binary_format::normalized::{Field, Type};
use move_binary_format::CompiledModule;
use move_command_line_common::files::FileHash;
use move_compiler::diagnostics::report_diagnostics_to_buffer;
use move_compiler::shared::files::{FileName, FilesSourceText};
use move_core_types::identifier::Identifier;
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
fn test_additive() {
    let (pkg_v1, pkg_v2, p) = get_packages("additive_errors");
    let result = compare_packages(pkg_v1, pkg_v2, p, UpgradePolicy::Additive);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}

#[test]
fn test_deponly() {
    let (pkg_v1, pkg_v2, p) = get_packages("deponly_errors");
    let result = compare_packages(pkg_v1, pkg_v2, p, UpgradePolicy::DepOnly);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_snapshot!(normalize_path(err.to_string()));
}
#[test]
fn test_version_mismatch() {
    // use deponly errors package, but change the version of the package and the module
    // to trigger _only_ a version mismatch error (not a deponly error)
    let (mut pkg_v1, mut pkg_v2, p) = get_packages("deponly_errors");
    pkg_v1[0].version = 1; // previous version was 1
    pkg_v2.package.root_compiled_units[0].unit.module.version = 0; // downgraded to version 0

    let result = compare_packages(pkg_v1, pkg_v2, p, UpgradePolicy::Additive);
    assert!(result.is_err());
    assert_snapshot!(normalize_path(result.unwrap_err().to_string()));
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
fn test_missing_module_toml() {
    // note: the first examples empty and whitespace shouldn't occur in practice
    // since a Move.toml which is empty will not build
    for malformed_pkg in [
        "emoji",
        "addresses_first",
        "starts_second_line",
        "package_no_name",
        "whitespace",
        "empty",
    ] {
        let move_pkg_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/unit_tests/fixtures/upgrade_errors/missing_module_toml/")
            .join(malformed_pkg);

        let move_toml_contents: Arc<str> = fs::read_to_string(move_pkg_path.join("Move.toml"))
            .unwrap_or_default()
            .into();
        let move_toml_hash = FileHash::new(&move_toml_contents);

        let result = missing_module_diag(
            &Identifier::from_str("identifier").unwrap(),
            &move_toml_hash,
            &move_toml_contents,
        );

        let move_toml: Arc<str> = fs::read_to_string(move_pkg_path.join("Move.toml"))
            .unwrap_or_default()
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
        assert_snapshot!(malformed_pkg, normalize_path(output));
    }
}

#[test]
fn positional_formatting() {
    let name = Identifier::new("pos999").unwrap();
    let field = Field {
        name,
        type_: Type::Bool,
    };

    let ff = FormattedField::new(&field, &[]);
    assert_eq!(format!("{}", ff), "'bool' at position 999");
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
    let re = regex::Regex::new(r"^(.*)┌─ .*(\/fixtures\/.*\.(move|toml):\d+:\d+)$").unwrap();
    err_string
        .lines()
        .map(|line| re.replace(line, "$1┌─ $2").into_owned())
        .collect::<Vec<String>>()
        .join("\n")
}
