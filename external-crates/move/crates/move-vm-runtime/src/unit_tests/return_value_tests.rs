// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units, serialize_module_at_max_version},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_arguments::ValueFrame,
        vm_test_adapter::VMTestAdapter,
    },
    execution::values::{Reference, Value},
    shared::gas::UnmeteredGasMeter,
};
use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, TypeTag},
    runtime_value::MoveValue,
};

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

fn run(
    structs: &[&str],
    fun_sig: &str,
    fun_body: &str,
    ty_arg_tags: Vec<TypeTag>,
    args: Vec<MoveValue>,
) -> VMResult<ValueFrame> {
    let structs = structs.to_vec().join("\n");

    let code = format!(
        r#"
        module 0x{}::M {{
            {}

            fun foo{} {{
                {}
            }}
        }}
    "#,
        TEST_ADDR, structs, fun_sig, fun_body
    );

    let mut units = compile_units(&code).unwrap();
    let m = as_module(units.pop().unwrap());
    let mut blob = vec![];
    serialize_module_at_max_version(&m, &mut blob).unwrap();

    let mut adapter = InMemoryTestAdapter::new();
    let pkg = StoredPackage::from_modules_for_testing(TEST_ADDR, vec![m]).unwrap();
    adapter.insert_package_into_storage(pkg);
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());
    let linkage = adapter.get_linkage_context(TEST_ADDR).unwrap();
    let mut sess = adapter.make_vm(linkage).unwrap();

    let fun_name = Identifier::new("foo").unwrap();

    let ty_args: Vec<_> = ty_arg_tags
        .into_iter()
        .map(|tag| sess.load_type(&tag))
        .collect::<VMResult<_>>()?;

    ValueFrame::serialized_call(
        &mut sess,
        &module_id,
        &fun_name,
        ty_args,
        args.into_iter()
            .map(|v| v.simple_serialize().unwrap())
            .collect(),
        &mut UnmeteredGasMeter,
        None,
        true, /* bypass_visibility_checks */
    )
}

fn expect_success(
    structs: &[&str],
    fun_sig: &str,
    fun_body: &str,
    ty_args: Vec<TypeTag>,
    args: Vec<MoveValue>,
    expected_values: &[MoveValue],
) {
    let return_vals = run(structs, fun_sig, fun_body, ty_args, args).unwrap();
    assert!(return_vals.values.len() == expected_values.len());

    for (ret_val, exp_val) in return_vals.values.iter().zip(expected_values.iter()) {
        assert_eq!(
            ret_val.serialize().unwrap(),
            exp_val.simple_serialize().unwrap()
        );
    }
}

#[test]
fn return_nothing() {
    expect_success(&[], "()", "", vec![], vec![], &[])
}

#[test]
fn return_u64() {
    expect_success(&[], "(): u64", "42", vec![], vec![], &[MoveValue::U64(42)])
}

#[test]
fn return_u64_bool() {
    expect_success(
        &[],
        "(): (u64, bool)",
        "(42, true)",
        vec![],
        vec![],
        &[MoveValue::U64(42), MoveValue::Bool(true)],
    )
}

#[test]
fn return_signer_ref() {
    let ValueFrame {
        heap: _,
        heap_mut_refs: _,
        heap_imm_refs: heap_refs,
        values,
    } = run(
        &[],
        "(s: &signer): &signer",
        "s",
        vec![],
        vec![MoveValue::Signer(TEST_ADDR)],
    )
    .unwrap();
    assert!(values.len() == 1);
    assert!(heap_refs.len() == 1);
    let Value::Reference(Reference::Value(inner)) = &values[0] else {
        panic!("Expected reference return");
    };
    let inner_val = inner.borrow();
    let ret_move_val =
        inner_val.as_move_value(&move_core_types::runtime_value::MoveTypeLayout::Signer);
    let expected = MoveValue::Signer(TEST_ADDR);
    assert_eq!(ret_move_val, expected);
}
