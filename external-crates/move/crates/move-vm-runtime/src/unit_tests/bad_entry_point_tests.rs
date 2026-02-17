// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    shared::{gas::UnmeteredGasMeter, linkage_context::LinkageContext},
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    vm_status::{StatusCode, StatusType},
};
use std::collections::BTreeMap;

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

#[test]
fn call_non_existent_module() {
    let adapter = InMemoryTestAdapter::new();
    let linkage = LinkageContext::new(BTreeMap::new());
    let mut vm = adapter.make_vm(linkage).unwrap();

    let before = adapter.runtime().cache().to_cache_telemetry().interner_size;
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();

    let err = vm
        .execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            vec![],
            &mut UnmeteredGasMeter,
            None,
        )
        .unwrap_err();
    let after = adapter.runtime().cache().to_cache_telemetry().interner_size;

    assert_eq!(err.status_type(), StatusType::Execution);
    assert_eq!(
        err.major_status(),
        StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR
    );
    assert_eq!(
        before, after,
        "Unexpected interner growth when calling non-existent module"
    );
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
    let pkg = StoredPackage::from_modules_for_testing(TEST_ADDR, vec![m]).unwrap();
    adapter.insert_package_into_storage(pkg);

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());
    let linkage = adapter.get_linkage_context(TEST_ADDR).unwrap();

    let mut sess = adapter.make_vm(linkage).unwrap();
    let before = adapter.runtime().cache().to_cache_telemetry().interner_size;

    let fun_name = Identifier::new("foo").unwrap();

    let err = sess
        .execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            vec![],
            &mut UnmeteredGasMeter,
            None,
        )
        .unwrap_err();
    let after = adapter.runtime().cache().to_cache_telemetry().interner_size;

    assert_eq!(err.status_type(), StatusType::Execution);
    assert_eq!(
        err.major_status(),
        StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR
    );
    assert_eq!(
        before, after,
        "Unexpected interner growth when calling non-existent module"
    );
}

#[test]
fn resolve_non_existent_type() {
    let code = r#"
        module {{ADDR}}::M {}
    "#;
    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));

    let mut units = compile_units(&code).unwrap();
    let m = as_module(units.pop().unwrap());

    let mut adapter = InMemoryTestAdapter::new();
    let pkg = StoredPackage::from_modules_for_testing(TEST_ADDR, vec![m]).unwrap();
    adapter.insert_package_into_storage(pkg);

    let linkage = adapter.get_linkage_context(TEST_ADDR).unwrap();

    let sess = adapter.make_vm(linkage).unwrap();
    let before = adapter.runtime().cache().to_cache_telemetry().interner_size;

    let err = sess
        .load_type(&TypeTag::Struct(Box::new(StructTag {
            address: TEST_ADDR,
            module: Identifier::new("M").unwrap(),
            name: Identifier::new("S").unwrap(),
            type_params: vec![],
        })))
        .unwrap_err();
    let after = adapter.runtime().cache().to_cache_telemetry().interner_size;

    assert_eq!(err.status_type(), StatusType::Execution);
    assert_eq!(
        err.major_status(),
        StatusCode::EXTERNAL_RESOLUTION_REQUEST_ERROR
    );
    assert_eq!(
        before, after,
        "Unexpected interner growth when calling non-existent module"
    );
}
