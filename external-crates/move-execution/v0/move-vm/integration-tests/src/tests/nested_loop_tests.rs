// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compiler::{as_module, as_script, compile_units};
use move_bytecode_verifier::VerifierConfig;
use move_core_types::account_address::AccountAddress;
use move_vm_runtime::{config::VMConfig, move_vm::MoveVM};
use move_vm_test_utils::InMemoryStorage;
use move_vm_types::gas::UnmeteredGasMeter;

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

#[test]
fn test_publish_module_with_nested_loops() {
    // Compile the modules and scripts.
    // TODO: find a better way to include the Signer module.
    let code = r#"
        module {{ADDR}}::M {
            fun foo() {
                let i = 0;
                while (i < 10) {
                    let j = 0;
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
    let mut m_blob = vec![];
    m.serialize(&mut m_blob).unwrap();

    // Should succeed with max_loop_depth = 2
    {
        let storage = InMemoryStorage::new();
        let vm = MoveVM::new_with_config(
            move_stdlib::natives::all_natives(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                move_stdlib::natives::GasParameters::zeros(),
            ),
            VMConfig {
                verifier: VerifierConfig {
                    max_loop_depth: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();

        let mut sess = vm.new_session(&storage);
        sess.publish_module(m_blob.clone(), TEST_ADDR, &mut UnmeteredGasMeter)
            .unwrap();
    }

    // Should fail with max_loop_depth = 1
    {
        let storage = InMemoryStorage::new();
        let vm = MoveVM::new_with_config(
            move_stdlib::natives::all_natives(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                move_stdlib::natives::GasParameters::zeros(),
            ),
            VMConfig {
                verifier: VerifierConfig {
                    max_loop_depth: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();

        let mut sess = vm.new_session(&storage);
        sess.publish_module(m_blob, TEST_ADDR, &mut UnmeteredGasMeter)
            .unwrap_err();
    }
}

#[test]
fn test_run_script_with_nested_loops() {
    // Compile the modules and scripts.
    // TODO: find a better way to include the Signer module.
    let code = r#"
        script {
            fun main() {
                let i = 0;
                while (i < 10) {
                    let j = 0;
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

    let s = as_script(units.pop().unwrap());
    let mut s_blob: Vec<u8> = vec![];
    s.serialize(&mut s_blob).unwrap();

    // Should succeed with max_loop_depth = 2
    {
        let storage = InMemoryStorage::new();
        let vm = MoveVM::new_with_config(
            move_stdlib::natives::all_natives(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                move_stdlib::natives::GasParameters::zeros(),
            ),
            VMConfig {
                verifier: VerifierConfig {
                    max_loop_depth: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();

        let mut sess = vm.new_session(&storage);
        let args: Vec<Vec<u8>> = vec![];
        sess.execute_script(s_blob.clone(), vec![], args, &mut UnmeteredGasMeter)
            .unwrap();
    }

    // Should fail with max_loop_depth = 1
    {
        let storage = InMemoryStorage::new();
        let vm = MoveVM::new_with_config(
            move_stdlib::natives::all_natives(
                AccountAddress::from_hex_literal("0x1").unwrap(),
                move_stdlib::natives::GasParameters::zeros(),
            ),
            VMConfig {
                verifier: VerifierConfig {
                    max_loop_depth: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();

        let mut sess = vm.new_session(&storage);
        let args: Vec<Vec<u8>> = vec![];
        sess.execute_script(s_blob, vec![], args, &mut UnmeteredGasMeter)
            .unwrap_err();
    }
}
