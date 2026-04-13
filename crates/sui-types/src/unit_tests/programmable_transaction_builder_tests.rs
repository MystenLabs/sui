// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};

use crate::base_types::{SuiAddress, random_object_ref};
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::transaction::Argument::Input;
use crate::transaction::{CallArg, Command, FundsWithdrawalArg, ObjectArg};

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

/// Example: Sweep all USDC (coin object + address balance) into a single coin and transfer it.
///
/// Scenario:
///   - Deposit address owns a USDC Coin object worth 5 USDC.
///   - Deposit address has 3 USDC sitting in its address balance (funds accumulator).
///   - We want to atomically redeem the address balance into a coin, merge both coins,
///     and transfer the resulting 8 USDC coin to address X (the sponsor / sweep destination).
///
/// The PTB does:
///   1. `coin::redeem_funds<USDC>(withdrawal)` — converts the FundsWithdrawal input into a Coin<USDC>
///   2. `MergeCoins` — merges the redeemed coin into the existing 5 USDC coin object
///   3. `TransferObjects` — sends the merged coin (now 8 USDC) to address X
#[test]
fn example_sweep_coin_and_address_balance() {
    // -- Setup: fake object refs and addresses --

    // The existing USDC Coin<T> object on the deposit address (5 USDC).
    let usdc_coin_ref = random_object_ref();

    // The sweep destination address.
    let sweep_destination: SuiAddress = SuiAddress::random_for_testing_only();

    // A stand-in type tag for USDC. In practice this would be the actual
    // published USDC type, e.g. `0xPKG::usdc::USDC`.
    let usdc_type_tag = TypeTag::Struct(Box::new(StructTag {
        address: AccountAddress::from_hex_literal("0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7").unwrap(),
        module: Identifier::new("usdc").unwrap(),
        name: Identifier::new("USDC").unwrap(),
        type_params: vec![],
    }));

    // Amount of USDC in the address balance (funds accumulator).
    let address_balance_amount: u64 = 3_000_000; // 3 USDC (6 decimals)

    // -- Build the PTB --

    let mut builder = ProgrammableTransactionBuilder::new();

    // Input 0: The FundsWithdrawal reservation for 3 USDC from the sender's address balance.
    let withdrawal_input = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
            address_balance_amount,
            usdc_type_tag.clone(),
        ))
        .unwrap();

    // Input 1: The existing USDC coin object (5 USDC).
    let coin_input = builder
        .obj(ObjectArg::ImmOrOwnedObject(usdc_coin_ref))
        .unwrap();

    // Command 0: coin::redeem_funds<USDC>(withdrawal) -> Coin<USDC>
    // This redeems the address balance withdrawal into a new Coin object.
    let redeemed_coin = builder.programmable_move_call(
        crate::SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec![usdc_type_tag],
        vec![withdrawal_input],
    );

    // Command 1: MergeCoins(coin_input, [redeemed_coin])
    // Merges the redeemed 3 USDC coin into the existing 5 USDC coin, giving 8 USDC total.
    builder.command(Command::MergeCoins(coin_input, vec![redeemed_coin]));

    // Command 2: TransferObjects([coin_input], sweep_destination)
    // Transfers the merged coin (now 8 USDC) to the sweep destination address.
    builder.transfer_arg(sweep_destination, coin_input);

    // Finalize
    let ptb = builder.finish();

    // -- Print the transaction for inspection --
    println!("=== Sweep PTB ===");
    println!("Inputs ({}):", ptb.inputs.len());
    for (i, input) in ptb.inputs.iter().enumerate() {
        println!("  [{i}] {input:?}");
    }
    println!("Commands ({}):", ptb.commands.len());
    for (i, cmd) in ptb.commands.iter().enumerate() {
        println!("  [{i}] {cmd:?}");
    }
    println!("Full transaction: {ptb:#?}");
}
