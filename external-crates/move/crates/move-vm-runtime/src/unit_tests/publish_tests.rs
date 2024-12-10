// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Simplifies logic around re-using ModuleIds.
#![allow(clippy::redundant_clone)]

use crate::{
    dev_utils::{
        compilation_utils::compile_packages_in_file,
        in_memory_test_adapter::InMemoryTestAdapter,
        vm_test_adapter::VMTestAdapter,
    },
    shared::gas::UnmeteredGasMeter,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::ModuleId,
};

#[test]
fn publish_package_no_init() {
    let package_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    let pkg = compile_packages_in_file("publish_init_module.move", &[])
        .pop()
        .expect("heh");

    let _ = adapter
        .publish_package(package_address, pkg.into_serialized_package())
        .expect("Failed verification");
}

#[test]
fn publish_package_and_initialize() {
    let package_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    let pkg = compile_packages_in_file("publish_init_module.move", &[])
        .pop()
        .expect("heh");

    let (verified_pkg, mut vm) = adapter
        .verify_package(package_address, pkg.into_serialized_package())
        .expect("Failed verification");

    let module_id = ModuleId::new(package_address, Identifier::new("a").unwrap());
    let function = Identifier::new("initialize").unwrap();
    vm.execute_function_bypass_visibility(
        &module_id,
        &function,
        vec![],
        Vec::<Vec<u8>>::new(),
        &mut UnmeteredGasMeter,
    )
    .expect("Executed initialize function");
    adapter
        .publish_verified_package(package_address, verified_pkg)
        .expect("Publish failed");
}

#[test]
fn publish_package_and_abort_initialize() {
    let package_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    let pkg = compile_packages_in_file("publish_init_abort_module.move", &[])
        .pop()
        .expect("heh");

    let (_verified_pkg, mut vm) = adapter
        .verify_package(package_address, pkg.into_serialized_package())
        .expect("Failed verification");

    let module_id = ModuleId::new(package_address, Identifier::new("a").unwrap());
    let function = Identifier::new("initialize").unwrap();
    vm.execute_function_bypass_visibility(
        &module_id,
        &function,
        vec![],
        Vec::<Vec<u8>>::new(),
        &mut UnmeteredGasMeter,
    )
    .expect_err("Executed initialize function that should have aborted");
}
