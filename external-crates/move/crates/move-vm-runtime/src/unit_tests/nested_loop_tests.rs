// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    runtime::MoveRuntime,
};
use move_core_types::account_address::AccountAddress;
use move_vm_config::{runtime::VMConfig, verifier::VerifierConfig};

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

#[test]
fn test_publish_module_with_nested_loops() {
    // Compile the modules and scripts.
    // TODO: find a better way to include the Signer module.
    let code = r#"
        module {{ADDR}}::M {
            fun foo() {
                let mut i = 0;
                while (i < 10) {
                    let mut j = 0;
                    while (j < 10) {
                        j = j + 1;
                    };
                    i = i + 1;
                };
            }
        }
    "#;
    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));
    let mut units = compile_units(&code).unwrap();

    let m = as_module(units.pop().unwrap());

    // Should succeed with max_loop_depth = 2
    {
        let runtime = MoveRuntime::new(
            crate::natives::move_stdlib::stdlib_native_functions(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                crate::natives::move_stdlib::GasParameters::zeros(),
                /* silent debug */ true,
            )
            .unwrap(),
            VMConfig {
                verifier: VerifierConfig {
                    max_loop_depth: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);
        let pkg = StoredPackage::from_modules_for_testing(TEST_ADDR, vec![m.clone()]).unwrap();
        adapter
            .publish_package(TEST_ADDR, pkg.into_serialized_package())
            .unwrap();
    }

    // Should fail with max_loop_depth = 1
    {
        let runtime = MoveRuntime::new(
            crate::natives::move_stdlib::stdlib_native_functions(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                crate::natives::move_stdlib::GasParameters::zeros(),
                /* silent debug */ true,
            )
            .unwrap(),
            VMConfig {
                verifier: VerifierConfig {
                    max_loop_depth: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);
        let pkg = StoredPackage::from_modules_for_testing(TEST_ADDR, vec![m.clone()]).unwrap();
        adapter
            .publish_package(TEST_ADDR, pkg.into_serialized_package())
            .unwrap_err();
    }
}
