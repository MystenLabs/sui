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
    execution::{interpreter::locals::BaseHeap, values::Value},
    shared::gas::UnmeteredGasMeter,
};
use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, TypeTag},
    u256::U256,
    vm_status::StatusCode,
};

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

fn run(
    ty_params: &[&str],
    params: &[&str],
    ty_arg_tags: Vec<TypeTag>,
    args: Vec<Value>,
) -> VMResult<()> {
    let ty_params = ty_params
        .iter()
        .map(|var| format!("{}: copy + drop", var))
        .collect::<Vec<_>>()
        .join(", ");
    let params = params
        .iter()
        .enumerate()
        .map(|(idx, ty)| format!("_x{}: {}", idx, ty))
        .collect::<Vec<_>>()
        .join(", ");

    let code = format!(
        r#"
        module 0x{}::M {{
            public struct Foo has copy, drop {{ x: u64 }}
            public struct Bar<T> has copy, drop {{ x: T }}

            fun foo<{}>({}) {{ }}
        }}
    "#,
        TEST_ADDR, ty_params, params
    );

    let mut units = compile_units(&code).unwrap();
    let m = as_module(units.pop().unwrap());

    let mut adapter = InMemoryTestAdapter::new();
    let pkg = StoredPackage::from_modules_for_testing(TEST_ADDR, vec![m.clone()]).unwrap();
    adapter.insert_package_into_storage(pkg);
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());

    let linkage = adapter.get_linkage_context(TEST_ADDR).unwrap();
    let mut sess = adapter.make_vm(linkage).unwrap();

    let fun_name = Identifier::new("foo").unwrap();

    let ty_args: Vec<_> = ty_arg_tags
        .into_iter()
        .map(|tag| sess.load_type(&tag))
        .collect::<VMResult<_>>()?;

    sess.execute_function_bypass_visibility(
        &module_id,
        &fun_name,
        ty_args,
        args,
        &mut UnmeteredGasMeter,
        None,
    )?;

    Ok(())
}

fn expect_err(params: &[&str], args: Vec<Value>, expected_status: StatusCode) {
    assert!(run(&[], params, vec![], args).unwrap_err().major_status() == expected_status);
}

fn expect_ok(params: &[&str], args: Vec<Value>) {
    run(&[], params, vec![], args).unwrap()
}

fn expect_err_generic(
    ty_params: &[&str],
    params: &[&str],
    ty_args: Vec<TypeTag>,
    args: Vec<Value>,
    expected_status: StatusCode,
) {
    assert!(
        run(ty_params, params, ty_args, args)
            .unwrap_err()
            .major_status()
            == expected_status
    );
}

fn expect_ok_generic(ty_params: &[&str], params: &[&str], ty_args: Vec<TypeTag>, args: Vec<Value>) {
    run(ty_params, params, ty_args, args).unwrap()
}

/// Helper: wrap a `Value` in an immutable reference via a `BaseHeap`.
fn make_ref(heap: &mut BaseHeap, value: Value) -> Value {
    let (_id, ref_val) = heap.allocate_and_borrow_loc(value).unwrap();
    ref_val
}

#[test]
fn expected_0_args_got_0() {
    expect_ok(&[], vec![])
}

#[test]
fn expected_0_args_got_1() {
    expect_err(
        &[],
        vec![Value::u64(0)],
        StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_1_arg_got_0() {
    expect_err(&["u64"], vec![], StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH)
}

#[test]
fn expected_2_arg_got_1() {
    expect_err(
        &["u64", "bool"],
        vec![Value::u64(0)],
        StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_2_arg_got_3() {
    expect_err(
        &["u64", "bool"],
        vec![Value::u64(0), Value::bool(true), Value::bool(false)],
        StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_u64_got_u64() {
    expect_ok(&["u64"], vec![Value::u64(0)])
}

#[test]
#[allow(non_snake_case)]
fn expected_Foo_got_Foo() {
    expect_ok(&["Foo"], vec![Value::make_struct(vec![Value::u64(0)])])
}

#[test]
fn expected_signer_ref_got_signer() {
    let mut heap = BaseHeap::new();
    let signer_ref = make_ref(&mut heap, Value::signer(TEST_ADDR));
    expect_ok(&["&signer"], vec![signer_ref])
}

#[test]
fn expected_u64_signer_ref_got_u64_signer() {
    let mut heap = BaseHeap::new();
    let signer_ref = make_ref(&mut heap, Value::signer(TEST_ADDR));
    expect_ok(&["u64", "&signer"], vec![Value::u64(0), signer_ref])
}

#[test]
fn param_type_u64_ref() {
    let mut heap = BaseHeap::new();
    let u64_ref = make_ref(&mut heap, Value::u64(0));
    expect_ok(&["&u64"], vec![u64_ref])
}

#[test]
#[allow(non_snake_case)]
fn expected_T__T_got_u64__u64() {
    expect_ok_generic(&["T"], &["T"], vec![TypeTag::U64], vec![Value::u64(0)])
}

#[test]
#[allow(non_snake_case)]
fn expected_A_B__A_u64_vector_B_got_u8_u128__u8_u64_vector_u128() {
    expect_ok_generic(
        &["A", "B"],
        &["A", "u64", "vector<B>"],
        vec![TypeTag::U8, TypeTag::U128],
        vec![Value::u8(0), Value::u64(0), Value::vector_u128(vec![0, 0])],
    )
}

#[test]
#[allow(non_snake_case)]
fn expected_A_B__A_u32_vector_B_got_u16_u256__u16_u32_vector_u256() {
    expect_ok_generic(
        &["A", "B"],
        &["A", "u32", "vector<B>"],
        vec![TypeTag::U16, TypeTag::U256],
        vec![
            Value::u16(0),
            Value::u32(0),
            Value::vector_u256(vec![U256::from(0u8), U256::from(0u8)]),
        ],
    )
}

#[test]
#[allow(non_snake_case)]
fn expected_T__Bar_T_got_bool__Bar_bool() {
    expect_ok_generic(
        &["T"],
        &["Bar<T>"],
        vec![TypeTag::Bool],
        vec![Value::make_struct(vec![Value::bool(false)])],
    )
}

#[test]
#[allow(non_snake_case)]
fn expected_T__T_got_bool__bool() {
    expect_ok_generic(
        &["T"],
        &["T"],
        vec![TypeTag::Bool],
        vec![Value::bool(false)],
    )
}

#[test]
#[allow(non_snake_case)]
fn expected_T__T_ref_got_u64__u64() {
    let mut heap = BaseHeap::new();
    let u64_ref = make_ref(&mut heap, Value::u64(0));
    expect_ok_generic(&["T"], &["&T"], vec![TypeTag::U64], vec![u64_ref])
}

#[test]
fn expected_1_ty_arg_got_0() {
    expect_err_generic(
        &["T"],
        &["T"],
        vec![],
        vec![Value::u64(0)],
        StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_1_ty_arg_got_2() {
    expect_err_generic(
        &["T"],
        &["T"],
        vec![TypeTag::U64, TypeTag::Bool],
        vec![Value::u64(0)],
        StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_0_ty_args_got_1() {
    expect_err_generic(
        &[],
        &["u64"],
        vec![TypeTag::U64],
        vec![Value::u64(0)],
        StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_2_ty_args_got_1() {
    expect_err_generic(
        &["A", "B"],
        &["A", "B"],
        vec![TypeTag::U64],
        vec![Value::u64(0), Value::bool(false)],
        StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_2_ty_args_got_3() {
    expect_err_generic(
        &["A", "B"],
        &["A", "B"],
        vec![TypeTag::U64, TypeTag::Bool, TypeTag::U8],
        vec![Value::u64(0), Value::bool(false)],
        StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH,
    )
}
