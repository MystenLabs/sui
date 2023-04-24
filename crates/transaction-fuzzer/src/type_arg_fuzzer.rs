// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use proptest::arbitrary::*;
use proptest::prelude::*;

use sui_types::messages::{TransactionData, TransactionKind};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{TypeTag, SUI_FRAMEWORK_OBJECT_ID};

use crate::account_universe::AccountCurrent;
use crate::executor::{assert_is_acceptable_result, Executor};

const GAS: u64 = 1_000_000;
const GAS_PRICE: u64 = 1;

pub fn gen_type_tag() -> impl Strategy<Value = TypeTag> {
    prop_oneof![
        2 => any::<TypeTag>(),
        1 => gen_nested_type_tag()
    ]
}

// Generate deep nested type tags
pub fn gen_nested_type_tag() -> impl Strategy<Value = TypeTag> {
    let leaf = prop_oneof![
        Just(TypeTag::Bool),
        Just(TypeTag::U8),
        Just(TypeTag::U16),
        Just(TypeTag::U32),
        Just(TypeTag::U64),
        Just(TypeTag::U128),
        Just(TypeTag::U256),
        Just(TypeTag::Address),
        Just(TypeTag::Signer),
    ];
    leaf.prop_recursive(8, 6, 10, |inner| {
        prop_oneof![
            inner.prop_map(|x| TypeTag::Vector(Box::new(x))),
            gen_struct_tag().prop_map(|x| TypeTag::Struct(Box::new(x))),
        ]
    })
}

pub fn gen_struct_tag() -> impl Strategy<Value = StructTag> {
    (
        any::<AccountAddress>(),
        any::<Identifier>(),
        any::<Identifier>(),
        any::<Vec<TypeTag>>(),
    )
        .prop_map(|(address, module, name, type_params)| StructTag {
            address,
            module,
            name,
            type_params,
        })
}

pub fn pt_for_tags(type_tags: Vec<TypeTag>) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            SUI_FRAMEWORK_OBJECT_ID,
            Identifier::new("random_type_tag_fuzzing").unwrap(),
            Identifier::new("random_type_tag_fuzzing_fn").unwrap(),
            type_tags,
            vec![],
        )
        .unwrap();
    TransactionKind::ProgrammableTransaction(builder.finish())
}

pub fn run_type_tags(account: &AccountCurrent, exec: &mut Executor, type_tags: Vec<TypeTag>) {
    let gas_object = account
        .current_coins
        .get(0)
        .unwrap()
        .compute_object_reference();
    let kind = pt_for_tags(type_tags);
    let tx_data = TransactionData::new(
        kind,
        account.initial_data.account.address,
        gas_object,
        GAS,
        GAS_PRICE,
    );
    let signed_txn = to_sender_signed_transaction(tx_data, &account.initial_data.account.key);
    let result = exec.execute_transaction(signed_txn);
    assert_is_acceptable_result(&result);
}
