// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use fs_extra::dir::CopyOptions;
use move_compiler::editions::Edition;

use crate::BuildConfig;

#[tokio::test]
async fn generate_struct_layouts() {
    // build the Sui framework and generate struct layouts to make sure nothing crashes
    let packages_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
        .join("sui-framework")
        .join("packages");

    // TODO dvx-1889: this might be unnecessary if we can run without regenerating lockfiles
    let tempdir = tempfile::tempdir().unwrap();
    fs_extra::dir::copy(
        &packages_path,
        &tempdir,
        &CopyOptions::new().content_only(true),
    )
    .unwrap();

    let pkg = BuildConfig::new_for_testing()
        .build_async(&tempdir.path().join("sui-framework"))
        .await
        .unwrap();

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

#[tokio::test]
async fn development_mode_not_allowed() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .to_path_buf()
        .join("src")
        .join("unit_tests")
        .join("data")
        .join("no_development_mode");
    let err = BuildConfig::new_for_testing()
        .build_async(&path)
        .await
        .expect_err("Should have failed due to unsupported edition");
    assert!(
        err.to_string()
            .contains(&Edition::DEVELOPMENT.unknown_edition_error().to_string())
    );
}
