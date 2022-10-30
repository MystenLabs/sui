// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::compiled_package::BuildConfig;

#[test]
fn generate_struct_layouts() {
    // build the Sui framework and generate struct layouts to make sure nothing crashes
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("sui-framework");
    let pkg = BuildConfig::default().build(path).unwrap();
    let registry = pkg.generate_struct_layouts();
    // check for a couple of types that aren't likely to go away
    assert!(registry.contains_key("0000000000000000000000000000000000000001::string::String"));
    assert!(registry.contains_key("0000000000000000000000000000000000000002::object::UID"));
    assert!(
        registry.contains_key("0000000000000000000000000000000000000002::tx_context::TxContext")
    );
}
