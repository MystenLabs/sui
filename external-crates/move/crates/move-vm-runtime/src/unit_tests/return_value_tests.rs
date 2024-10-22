// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dev_utils::{
        compilation_utils::{as_module, compile_units, serialize_module_at_max_version},
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    shared::{gas::UnmeteredGasMeter, serialization::SerializedReturnValues},
};
use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, TypeTag},
    runtime_value::{MoveTypeLayout, MoveValue},
};

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

fn run(
    structs: &[&str],
    fun_sig: &str,
    fun_body: &str,
    ty_arg_tags: Vec<TypeTag>,
    args: Vec<MoveValue>,
) -> VMResult<Vec<Vec<u8>>> {
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

    let args: Vec<_> = args
        .into_iter()
        .map(|val| val.simple_serialize().unwrap())
        .collect();

    let SerializedReturnValues {
        return_values,
        mutable_reference_outputs: _,
    } = sess.execute_function_bypass_visibility(
        &module_id,
        &fun_name,
        ty_args,
        args,
        &mut UnmeteredGasMeter,
    )?;

    Ok(return_values
        .into_iter()
        .map(|(bytes, _layout)| bytes)
        .collect())
}

fn expect_success(
    structs: &[&str],
    fun_sig: &str,
    fun_body: &str,
    ty_args: Vec<TypeTag>,
    args: Vec<MoveValue>,
    expected_layouts: &[MoveTypeLayout],
) {
    let return_vals = run(structs, fun_sig, fun_body, ty_args, args).unwrap();
    assert!(return_vals.len() == expected_layouts.len());

    for (blob, layout) in return_vals.iter().zip(expected_layouts.iter()) {
        MoveValue::simple_deserialize(blob, layout).unwrap();
    }
}

#[test]
fn return_nothing() {
    expect_success(&[], "()", "", vec![], vec![], &[])
}

#[test]
fn return_u64() {
    expect_success(&[], "(): u64", "42", vec![], vec![], &[MoveTypeLayout::U64])
}

#[test]
fn return_u64_bool() {
    expect_success(
        &[],
        "(): (u64, bool)",
        "(42, true)",
        vec![],
        vec![],
        &[MoveTypeLayout::U64, MoveTypeLayout::Bool],
    )
}

#[test]
fn return_signer_ref() {
    expect_success(
        &[],
        "(s: &signer): &signer",
        "s",
        vec![],
        vec![MoveValue::Signer(TEST_ADDR)],
        &[MoveTypeLayout::Signer],
    )
}
