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
    shared::gas::UnmeteredGasMeter,
};
use move_binary_format::errors::VMResult;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, TypeTag},
    runtime_value::{MoveStruct, MoveValue},
    u256::U256,
    vm_status::StatusCode,
};

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

fn run(
    ty_params: &[&str],
    params: &[&str],
    ty_arg_tags: Vec<TypeTag>,
    args: Vec<MoveValue>,
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
        true,
    )?;

    Ok(())
}

fn expect_err(params: &[&str], args: Vec<MoveValue>, expected_status: StatusCode) {
    assert!(run(&[], params, vec![], args).unwrap_err().major_status() == expected_status);
}

fn expect_err_generic(
    ty_params: &[&str],
    params: &[&str],
    ty_args: Vec<TypeTag>,
    args: Vec<MoveValue>,
    expected_status: StatusCode,
) {
    assert!(
        run(ty_params, params, ty_args, args)
            .unwrap_err()
            .major_status()
            == expected_status
    );
}

fn expect_ok(params: &[&str], args: Vec<MoveValue>) {
    run(&[], params, vec![], args).unwrap()
}

fn expect_ok_generic(
    ty_params: &[&str],
    params: &[&str],
    ty_args: Vec<TypeTag>,
    args: Vec<MoveValue>,
) {
    run(ty_params, params, ty_args, args).unwrap()
}

#[test]
fn expected_0_args_got_0() {
    expect_ok(&[], vec![])
}

#[test]
fn expected_0_args_got_1() {
    expect_err(
        &[],
        vec![MoveValue::U64(0)],
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
        vec![MoveValue::U64(0)],
        StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_2_arg_got_3() {
    expect_err(
        &["u64", "bool"],
        vec![
            MoveValue::U64(0),
            MoveValue::Bool(true),
            MoveValue::Bool(false),
        ],
        StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
    )
}

#[test]
fn expected_u64_got_u64() {
    expect_ok(&["u64"], vec![MoveValue::U64(0)])
}

#[test]
#[allow(non_snake_case)]
fn expected_Foo_got_Foo() {
    expect_ok(
        &["Foo"],
        vec![MoveValue::Struct(MoveStruct::new(vec![MoveValue::U64(0)]))],
    )
}

#[test]
fn expected_signer_ref_got_signer() {
    expect_ok(&["&signer"], vec![MoveValue::Signer(TEST_ADDR)])
}

#[test]
fn expected_u64_signer_ref_got_u64_signer() {
    expect_ok(
        &["u64", "&signer"],
        vec![MoveValue::U64(0), MoveValue::Signer(TEST_ADDR)],
    )
}

#[test]
fn expected_u64_got_bool() {
    expect_err(
        &["u64"],
        vec![MoveValue::Bool(false)],
        StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
    )
}

#[test]
fn param_type_u64_ref() {
    expect_ok(&["&u64"], vec![MoveValue::U64(0)])
}

#[test]
#[allow(non_snake_case)]
fn expected_T__T_got_u64__u64() {
    expect_ok_generic(&["T"], &["T"], vec![TypeTag::U64], vec![MoveValue::U64(0)])
}

#[test]
#[allow(non_snake_case)]
fn expected_A_B__A_u64_vector_B_got_u8_u128__u8_u64_vector_u128() {
    expect_ok_generic(
        &["A", "B"],
        &["A", "u64", "vector<B>"],
        vec![TypeTag::U8, TypeTag::U128],
        vec![
            MoveValue::U8(0),
            MoveValue::U64(0),
            MoveValue::Vector(vec![MoveValue::U128(0), MoveValue::U128(0)]),
        ],
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
            MoveValue::U16(0),
            MoveValue::U32(0),
            MoveValue::Vector(vec![
                MoveValue::U256(U256::from(0u8)),
                MoveValue::U256(U256::from(0u8)),
            ]),
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
        vec![MoveValue::Struct(MoveStruct::new(vec![MoveValue::Bool(
            false,
        )]))],
    )
}

#[test]
#[allow(non_snake_case)]
fn expected_T__T_got_bool__bool() {
    expect_ok_generic(
        &["T"],
        &["T"],
        vec![TypeTag::Bool],
        vec![MoveValue::Bool(false)],
    )
}

#[test]
#[allow(non_snake_case)]
fn expected_T__T_got_bool__u64() {
    expect_err_generic(
        &["T"],
        &["T"],
        vec![TypeTag::Bool],
        vec![MoveValue::U64(0)],
        StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
    )
}

#[test]
#[allow(non_snake_case)]
fn expected_T__T_ref_got_u64__u64() {
    expect_ok_generic(&["T"], &["&T"], vec![TypeTag::U64], vec![MoveValue::U64(0)])
}

#[test]
#[allow(non_snake_case)]
fn expected_T__Bar_T_got_bool__Bar_u64() {
    expect_err_generic(
        &["T"],
        &["Bar<T>"],
        vec![TypeTag::Bool],
        vec![MoveValue::Struct(MoveStruct::new(vec![MoveValue::U64(0)]))],
        StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
    )
}
