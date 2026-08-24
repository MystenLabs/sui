// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        in_memory_test_adapter::InMemoryTestAdapter, storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    natives::move_stdlib::{GasParameters, stdlib_native_functions},
    runtime::MoveRuntime,
};
use move_binary_format::{file_format::basic_test_module, file_format_common::VERSION_MAX};
use move_core_types::{account_address::AccountAddress, vm_status::StatusCode};
use move_vm_config::runtime::VMConfig;

#[test]
fn test_publish_module_with_custom_max_binary_format_version() {
    let m = basic_test_module();

    // Should accept both modules with the default settings
    {
        let move_runtime = MoveRuntime::new_with_default_config(
            stdlib_native_functions(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                GasParameters::zeros(),
                /* silent debug */ true,
            )
            .unwrap(),
        );
        let mut adapter = InMemoryTestAdapter::new_with_runtime(move_runtime);

        // Publish package with version VERSION_MAX -- should succeed
        {
            let mut b_new = m.clone();
            b_new.version = VERSION_MAX;
            b_new.address_identifiers[0] = AccountAddress::from_hex_literal("0x2").unwrap();
            let addr = *b_new.self_id().address();

            let b_new_pkg =
                StoredPackage::from_modules_for_testing(addr, vec![b_new.clone()]).unwrap();
            adapter
                .publish_package(addr, b_new_pkg.into_serialized_package())
                .unwrap();
        }

        // Publish package with version VERSION_MAX - 1 -- should succeed
        {
            let mut b_old = m.clone();
            b_old.version = VERSION_MAX.checked_sub(1).unwrap();
            b_old.address_identifiers[0] = AccountAddress::from_hex_literal("0x3").unwrap();
            let addr = *b_old.self_id().address();

            let b_old_pkg =
                StoredPackage::from_modules_for_testing(addr, vec![b_old.clone()]).unwrap();
            adapter
                .publish_package(addr, b_old_pkg.into_serialized_package())
                .unwrap();
        }
    }

    // Should reject the module with newer version with max binary format version being set to VERSION_MAX - 1
    {
        let mut vm_config = VMConfig::default();
        // lower the max version allowed
        let max_updated = VERSION_MAX.checked_sub(1).unwrap();
        vm_config.max_binary_format_version = max_updated;
        vm_config.binary_config.max_binary_format_version = max_updated;

        let move_runtime = MoveRuntime::new(
            stdlib_native_functions(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                GasParameters::zeros(),
                /* silent debug */ true,
            )
            .unwrap(),
            vm_config,
        );
        let mut adapter = InMemoryTestAdapter::new_with_runtime(move_runtime);

        // Publish package with version VERSION_MAX -- should fail
        {
            let mut b_new = m.clone();
            b_new.version = VERSION_MAX;
            b_new.address_identifiers[0] = AccountAddress::from_hex_literal("0x2").unwrap();
            let addr = *b_new.self_id().address();

            let b_new_pkg =
                StoredPackage::from_modules_for_testing(addr, vec![b_new.clone()]).unwrap();
            let err = adapter
                .publish_package(addr, b_new_pkg.into_serialized_package())
                .unwrap_err();
            assert_eq!(err.major_status(), StatusCode::UNKNOWN_VERSION);
        }

        // Publish package with version VERSION_MAX - 1 -- should succeed
        {
            let mut b_old = m.clone();
            b_old.version = VERSION_MAX.checked_sub(1).unwrap();
            b_old.address_identifiers[0] = AccountAddress::from_hex_literal("0x3").unwrap();
            let addr = *b_old.self_id().address();

            let b_old_pkg =
                StoredPackage::from_modules_for_testing(addr, vec![b_old.clone()]).unwrap();
            adapter
                .publish_package(addr, b_old_pkg.into_serialized_package())
                .unwrap();
        }
    }
}
