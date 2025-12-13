// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::random_object_ref;
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::transaction::Argument::{Input, NestedResult};
use crate::transaction::{CallArg, Command, ObjectArg};
use move_core_types::{identifier::Identifier, language_storage::TypeTag};

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

#[test]
fn test_command_with_multiple_results_n1() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();
    let coin_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(target_coin_ref))
        .unwrap();
    let amt_arg = builder.pure(100u64).unwrap();

    let [result] =
        builder.command_with_multiple_results::<1>(Command::SplitCoins(coin_arg, vec![amt_arg]));

    assert_eq!(result, NestedResult(0, 0));

    let tx = builder.finish();
    assert_eq!(tx.commands.len(), 1);
}

#[test]
fn test_command_with_multiple_results_n2() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();
    let coin_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(target_coin_ref))
        .unwrap();
    let amt1_arg = builder.pure(100u64).unwrap();
    let amt2_arg = builder.pure(200u64).unwrap();

    let [result1, result2] = builder.command_with_multiple_results::<2>(Command::SplitCoins(
        coin_arg,
        vec![amt1_arg, amt2_arg],
    ));

    assert_eq!(result1, NestedResult(0, 0));
    assert_eq!(result2, NestedResult(0, 1));

    let tx = builder.finish();
    assert_eq!(tx.commands.len(), 1);
}

#[test]
fn test_command_with_multiple_results_n3() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let target_coin_ref = random_object_ref();
    let coin_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(target_coin_ref))
        .unwrap();
    let amt1_arg = builder.pure(100u64).unwrap();
    let amt2_arg = builder.pure(200u64).unwrap();
    let amt3_arg = builder.pure(300u64).unwrap();

    let [result1, result2, result3] = builder.command_with_multiple_results::<3>(
        Command::SplitCoins(coin_arg, vec![amt1_arg, amt2_arg, amt3_arg]),
    );

    assert_eq!(result1, NestedResult(0, 0));
    assert_eq!(result2, NestedResult(0, 1));
    assert_eq!(result3, NestedResult(0, 2));

    let tx = builder.finish();
    assert_eq!(tx.commands.len(), 1);
}

#[test]
fn test_programmable_move_call_with_multiple_results_n2() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let package = random_object_ref().0;
    let module = Identifier::new("test_module").unwrap();
    let function = Identifier::new("test_function").unwrap();
    let arg1 = builder.pure(42u64).unwrap();
    let arg2 = builder.pure(100u64).unwrap();

    let [result1, result2] = builder.programmable_move_call_with_multiple_results::<2>(
        package,
        module,
        function,
        vec![],
        vec![arg1, arg2],
    );

    assert_eq!(result1, NestedResult(0, 0));
    assert_eq!(result2, NestedResult(0, 1));

    let tx = builder.finish();
    assert_eq!(tx.commands.len(), 1);
    match &tx.commands[0] {
        Command::MoveCall(call) => {
            assert_eq!(call.arguments, vec![arg1, arg2]);
        }
        _ => panic!("Expected MoveCall command"),
    }
}

#[test]
fn test_programmable_move_call_with_multiple_results_n3() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let package = random_object_ref().0;
    let module = Identifier::new("test_module").unwrap();
    let function = Identifier::new("test_function").unwrap();
    let type_arg = TypeTag::U64;
    let arg = builder.pure(42u64).unwrap();

    let [result1, result2, result3] = builder.programmable_move_call_with_multiple_results::<3>(
        package,
        module,
        function,
        vec![type_arg],
        vec![arg],
    );

    assert_eq!(result1, NestedResult(0, 0));
    assert_eq!(result2, NestedResult(0, 1));
    assert_eq!(result3, NestedResult(0, 2));

    let tx = builder.finish();
    assert_eq!(tx.commands.len(), 1);
}

#[test]
fn test_command_with_multiple_results_sequential_commands() {
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin1_ref = random_object_ref();
    let coin2_ref = random_object_ref();
    let coin1_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin1_ref)).unwrap();
    let coin2_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin2_ref)).unwrap();
    let amt1_arg = builder.pure(100u64).unwrap();
    let amt2_arg = builder.pure(200u64).unwrap();

    let [split1_result1, split1_result2] =
        builder.command_with_multiple_results::<2>(Command::SplitCoins(coin1_arg, vec![amt1_arg]));

    let [split2_result1] =
        builder.command_with_multiple_results::<1>(Command::SplitCoins(coin2_arg, vec![amt2_arg]));

    assert_eq!(split1_result1, NestedResult(0, 0));
    assert_eq!(split1_result2, NestedResult(0, 1));
    assert_eq!(split2_result1, NestedResult(1, 0));

    let tx = builder.finish();
    assert_eq!(tx.commands.len(), 2);
}
