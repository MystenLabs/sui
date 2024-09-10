// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{file_format::basic_test_module, file_format_common::VERSION_MAX};
use move_core_types::{account_address::AccountAddress, vm_status::StatusCode};
use move_vm_config::runtime::VMConfig;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::InMemoryStorage;
use move_vm_types::gas::UnmeteredGasMeter;

#[test]
fn test_publish_module_with_custom_max_binary_format_version() {
    let m = basic_test_module();
    let mut b_new = vec![];
    let mut b_old = vec![];
    m.serialize_with_version(VERSION_MAX, &mut b_new).unwrap();
    m.serialize_with_version(VERSION_MAX.checked_sub(1).unwrap(), &mut b_old)
        .unwrap();

    // Should accept both modules with the default settings
    {
        let storage = InMemoryStorage::new();
        let vm = MoveVM::new(move_stdlib_natives::all_natives(
            AccountAddress::from_hex_literal("0x1").unwrap(),
            move_stdlib_natives::GasParameters::zeros(),
            /* silent debug */ true,
        ))
        .unwrap();
        let mut sess = vm.new_session(&storage);

        sess.publish_module(
            b_new.clone(),
            *m.self_id().address(),
            &mut UnmeteredGasMeter,
        )
        .unwrap();

        sess.publish_module(
            b_old.clone(),
            *m.self_id().address(),
            &mut UnmeteredGasMeter,
        )
        .unwrap();
    }

    // Should reject the module with newer version with max binary format version being set to VERSION_MAX - 1
    {
        let storage = InMemoryStorage::new();
        let mut vm_config = VMConfig::default();
        // lower the max version allowed
        let max_updated = VERSION_MAX.checked_sub(1).unwrap();
        vm_config.max_binary_format_version = max_updated;
        vm_config.binary_config.max_binary_format_version = max_updated;

        let vm = MoveVM::new_with_config(
            move_stdlib_natives::all_natives(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                move_stdlib_natives::GasParameters::zeros(),
                /* silent debug */ true,
            ),
            vm_config,
        )
        .unwrap();
        let mut sess = vm.new_session(&storage);

        assert_eq!(
            sess.publish_module(
                b_new.clone(),
                *m.self_id().address(),
                &mut UnmeteredGasMeter,
            )
            .unwrap_err()
            .major_status(),
            StatusCode::UNKNOWN_VERSION
        );

        sess.publish_module(
            b_old.clone(),
            *m.self_id().address(),
            &mut UnmeteredGasMeter,
        )
        .unwrap();
    }
}
