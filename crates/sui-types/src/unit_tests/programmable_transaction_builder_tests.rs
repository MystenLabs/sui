// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::random_object_ref;
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::transaction::Argument::Input;
use crate::transaction::{CallArg, Command, ObjectArg};

#[test]
fn test_builder_merge_coins_one_source() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();
    let coins_ref = random_object_ref();

    builder
        .merge_coins(target_coin_ref, vec![coins_ref])
        .unwrap();

    let tx = builder.finish();

    assert_eq!(
        tx.inputs,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(target_coin_ref)),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coins_ref))
        ]
    );
    assert_eq!(
        tx.commands,
        vec![Command::MergeCoins(Input(0), vec![Input(1)])]
    );
}

#[test]
fn test_builder_merge_coins_two_sources() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();
    let source_coin1_ref = random_object_ref();
    let source_coin2_ref = random_object_ref();

    builder
        .merge_coins(target_coin_ref, vec![source_coin1_ref, source_coin2_ref])
        .unwrap();

    let tx = builder.finish();

    assert_eq!(
        tx.inputs,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(target_coin_ref)),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(source_coin1_ref)),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(source_coin2_ref)),
        ]
    );
    assert_eq!(
        tx.commands,
        vec![Command::MergeCoins(Input(0), vec![Input(1), Input(2),])]
    );
}

#[test]
fn test_builder_merge_coins_zero_source() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();

    builder.merge_coins(target_coin_ref, vec![]).unwrap();

    let tx = builder.finish();

    assert_eq!(
        tx.inputs,
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
            target_coin_ref
        )),]
    );
    assert_eq!(tx.commands, vec![Command::MergeCoins(Input(0), vec![])]);
}

#[test]
fn test_builder_smash_coins_one_coin() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();

    let arg = builder.smash_coins(vec![target_coin_ref]).unwrap();

    let tx = builder.finish();

    assert_eq!(arg, Input(0));
    assert_eq!(
        tx.inputs,
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
            target_coin_ref
        ))]
    );
    assert_eq!(tx.commands, vec![Command::MergeCoins(Input(0), vec![])]);
}

#[test]
fn test_builder_smash_coins_two_coins() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();
    let source_coin_ref = random_object_ref();

    let arg = builder
        .smash_coins(vec![target_coin_ref, source_coin_ref])
        .unwrap();

    let tx = builder.finish();

    assert_eq!(arg, Input(0));
    assert_eq!(
        tx.inputs,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(target_coin_ref)),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(source_coin_ref))
        ]
    );
    assert_eq!(
        tx.commands,
        vec![Command::MergeCoins(Input(0), vec![Input(1)])]
    );
}

#[test]
fn test_builder_smash_coins_three_coin() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();
    let source_coin1_ref = random_object_ref();
    let source_coin2_ref = random_object_ref();

    let arg = builder
        .smash_coins(vec![target_coin_ref, source_coin1_ref, source_coin2_ref])
        .unwrap();

    let tx = builder.finish();

    assert_eq!(arg, Input(0));
    assert_eq!(
        tx.inputs,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(target_coin_ref)),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(source_coin1_ref)),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(source_coin2_ref))
        ]
    );
    assert_eq!(
        tx.commands,
        vec![Command::MergeCoins(Input(0), vec![Input(1), Input(2)])]
    );
}

#[test]
fn test_builder_smash_coins_zero_coin() {
    let mut builder = ProgrammableTransactionBuilder::new();

    let result = builder.smash_coins(vec![]);

    assert!(result.is_err());
}
