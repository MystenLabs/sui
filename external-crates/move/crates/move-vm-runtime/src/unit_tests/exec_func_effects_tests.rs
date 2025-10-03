// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_arguments::ValueFrame,
        vm_test_adapter::VMTestAdapter,
    },
    execution::values::Value,
    shared::gas::UnmeteredGasMeter,
};
use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, language_storage::ModuleId,
    runtime_value::MoveValue, u256::U256, vm_status::StatusCode,
};

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);
const TEST_MODULE_ID: &str = "M";
const EXPECT_MUTREF_OUT_VALUE: u64 = 90;
const USE_MUTREF_LABEL: &str = "use_mutref";
const USE_REF_LABEL: &str = "use_ref";
const FUN_NAMES: [&str; 2] = [USE_MUTREF_LABEL, USE_REF_LABEL];

// ensure proper errors are returned when ref & mut ref args fail to deserialize
#[test]
fn fail_arg_deserialize() {
    let mod_code = setup_module();
    // all of these should fail to deserialize because the functions expect u64 args
    let values = vec![
        MoveValue::U8(16),
        MoveValue::U16(1006),
        MoveValue::U32(16000),
        MoveValue::U128(512),
        MoveValue::U256(U256::from(12345u32)),
        MoveValue::Bool(true),
    ];
    for value in values {
        for name in FUN_NAMES {
            let err = run(&mod_code, name, value.clone())
                .map(|_| ())
                .expect_err("Should have failed to deserialize non-u64 type to u64");
            assert_eq!(
                err.major_status(),
                StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT
            );
        }
    }
}

// check happy path for writing to mut ref args - may be unecessary / covered by other tests
#[test]
fn mutref_output_success() {
    let mod_code = setup_module();
    let base_val = MoveValue::U64(1);
    let result = run(&mod_code, USE_MUTREF_LABEL, base_val);
    let mut ret_values = result.unwrap();
    assert!(ret_values.values.is_empty());
    assert!(ret_values.heap_mut_refs.len() == 1);
    let id = *ret_values.heap_mut_refs.iter().next().unwrap().1;
    let v = ret_values.heap.take_loc(id).unwrap();
    assert!(matches!(v, Value::U64(EXPECT_MUTREF_OUT_VALUE)));
}

// TODO - how can we cause serialization errors in values returned from Move ?
// that would allow us to test error paths for outputs as well

fn setup_module() -> ModuleCode {
    // first function takes a mutable ref & writes to it, the other takes immutable ref, so we exercise both paths
    let code = format!(
        r#"
        module 0x{}::{} {{
            fun {}(a: &mut u64) {{ *a = {}; }}
            fun {}(_a: & u64) {{ }}
        }}
    "#,
        TEST_ADDR, TEST_MODULE_ID, USE_MUTREF_LABEL, EXPECT_MUTREF_OUT_VALUE, USE_REF_LABEL
    );

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new(TEST_MODULE_ID).unwrap());
    (module_id, code)
}

fn run(module: &ModuleCode, fun_name: &str, arg_val0: MoveValue) -> VMResult<ValueFrame> {
    let module_id = &module.0;
    let modules = vec![module.clone()];
    let adapter = setup_vm(&modules);
    let linkage = adapter.get_linkage_context(*module_id.address()).unwrap();
    let mut session = adapter.make_vm(linkage).unwrap();

    let fun_name = Identifier::new(fun_name).unwrap();
    ValueFrame::serialized_call(
        &mut session,
        module_id,
        &fun_name,
        vec![],
        vec![arg_val0.simple_serialize().unwrap()],
        &mut UnmeteredGasMeter,
        None,
        true,
    )
}

type ModuleCode = (ModuleId, String);

// TODO - move some utility functions to where test infra lives, see about unifying with similar code
fn setup_vm(modules: &[ModuleCode]) -> InMemoryTestAdapter {
    let mut adapter = InMemoryTestAdapter::new();
    let modules: Vec<_> = modules
        .iter()
        .map(|(_, code)| {
            let mut units = compile_units(code).unwrap();
            as_module(units.pop().unwrap())
        })
        .collect();
    adapter.insert_package_into_storage(
        StoredPackage::from_modules_for_testing(*modules.first().unwrap().address(), modules)
            .unwrap(),
    );
    adapter
}
