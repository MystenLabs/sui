// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use move_compiler::editions::Edition;

use crate::BuildConfig;

#[test]
fn generate_struct_layouts() {
    // build the Sui framework and generate struct layouts to make sure nothing crashes
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
        .join("sui-framework")
        .join("packages")
        .join("sui-framework");
    let pkg = BuildConfig::new_for_testing().build(&path).unwrap();
    let registry = pkg.generate_struct_layouts();
    // check for a couple of types that aren't likely to go away
    assert!(registry.contains_key(
        "0000000000000000000000000000000000000000000000000000000000000001::string::String"
    ));
    assert!(registry.contains_key(
        "0000000000000000000000000000000000000000000000000000000000000002::object::UID"
    ));
    assert!(registry.contains_key(
        "0000000000000000000000000000000000000000000000000000000000000002::tx_context::TxContext"
    ));
}

#[test]
fn development_mode_not_allowed() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .to_path_buf()
        .join("src")
        .join("unit_tests")
        .join("data")
        .join("no_development_mode");
    let err = BuildConfig::new_for_testing()
        .build(&path)
        .expect_err("Should have failed due to unsupported edition");
    assert!(err
        .to_string()
        .contains(&Edition::DEVELOPMENT.unknown_edition_error().to_string()));
}
