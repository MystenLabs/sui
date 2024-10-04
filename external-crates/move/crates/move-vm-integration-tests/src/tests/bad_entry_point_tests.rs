// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compiler::{as_module, compile_units};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::ModuleId,
    runtime_value::{serialize_values, MoveValue},
    vm_status::StatusType,
};
use move_vm_runtime::{
    dev_utils::{in_memory_test_adapter::InMemoryTestAdapter, vm_test_adapter::VMTestAdapter},
    shared::{gas::UnmeteredGasMeter, linkage_context::LinkageContext},
};
use std::collections::HashMap;

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

#[test]
fn call_non_existent_module() {
    let adapter = InMemoryTestAdapter::new();
    let linkage = LinkageContext::new(TEST_ADDR, HashMap::new());
    let mut vm = adapter.make_vm(linkage).unwrap();

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();

    let err = vm
        .execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            serialize_values(&vec![MoveValue::Signer(TEST_ADDR)]),
            &mut UnmeteredGasMeter,
        )
        .unwrap_err();

    assert_eq!(err.status_type(), StatusType::Verification);
}

#[test]
fn call_non_existent_function() {
    let code = r#"
        module {{ADDR}}::M {}
    "#;
    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));

    let mut units = compile_units(&code).unwrap();
    let m = as_module(units.pop().unwrap());

    let mut adapter = InMemoryTestAdapter::new();
    adapter.insert_modules_into_storage(vec![m]).unwrap();

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());
    let linkage = adapter.generate_default_linkage(TEST_ADDR).unwrap();

    let mut sess = adapter.make_vm(linkage).unwrap();

    let fun_name = Identifier::new("foo").unwrap();

    let err = sess
        .execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            serialize_values(&vec![MoveValue::Signer(TEST_ADDR)]),
            &mut UnmeteredGasMeter,
        )
        .unwrap_err();

    assert_eq!(err.status_type(), StatusType::Verification);
}
