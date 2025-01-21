// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::compile_packages_in_file, in_memory_test_adapter::InMemoryTestAdapter,
        vm_test_adapter::VMTestAdapter,
    },
    jit::optimization,
};
use move_core_types::account_address::AccountAddress;

#[test]
fn test_basic_blocks_0() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("basic_blocks.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }
    let serialized_package = adapter.get_package_from_store(&package_address).unwrap();
    let (verif_pkg, _) = adapter
        .verify_package(package_address, serialized_package)
        .unwrap();
    let pkg = optimization::to_optimized_form(verif_pkg);
    println!("Blocks\n---------------------------\n{:#?}", pkg.modules);
}
