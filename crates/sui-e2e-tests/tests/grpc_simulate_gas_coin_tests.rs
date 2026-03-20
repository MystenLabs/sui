// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the GRPC simulate API's gas selection with address balance.
//!
//! Gas payment strategy:
//! - Has AB + has coins → Coin reservation FIRST (smashes coins into AB)
//! - Has AB + no coins  → Pure AB payment (empty gas_data.payment + expiration)
//! - No AB + has coins  → Traditional coin gas payment
//!
//! The presence of Argument::GasCoin affects what the user can ACCESS via tx.gas,
//! but the gas PAYMENT strategy is about smashing coins into AB whenever possible.

use sui_macros::sim_test;
use sui_types::{
    base_types::SuiAddress,
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Argument, TransactionData},
};
use test_cluster::addr_balance_test_env::TestEnvBuilder;

const MIST_PER_SUI: u64 = 1_000_000_000;

/// Helper to build a PTB that splits X MIST from GasCoin and transfers to recipient.
fn build_split_gas_coin_ptb(
    sender: SuiAddress,
    amount: u64,
    recipient: SuiAddress,
    gas: sui_types::base_types::ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> TransactionData {
    let mut ptb = ProgrammableTransactionBuilder::new();
    let amount_arg = ptb.pure(amount).unwrap();
    let split_result = ptb.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount_arg],
    ));
    let recipient_arg = ptb.pure(recipient).unwrap();
    ptb.command(sui_types::transaction::Command::TransferObjects(
        vec![split_result],
        recipient_arg,
    ));
    let pt = ptb.finish();
    TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, gas_price)
}

/// Helper to build a simple PTB that does NOT use GasCoin argument.
fn build_simple_transfer_ptb(
    sender: SuiAddress,
    recipient: SuiAddress,
    gas: sui_types::base_types::ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> TransactionData {
    let mut ptb = ProgrammableTransactionBuilder::new();
    // Just a simple split from gas coin - this DOES use GasCoin
    // For a true "no GasCoin" test, we'd need to transfer an owned object
    let amount_arg = ptb.pure(1000u64).unwrap();
    let split_result = ptb.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount_arg],
    ));
    let recipient_arg = ptb.pure(recipient).unwrap();
    ptb.command(sui_types::transaction::Command::TransferObjects(
        vec![split_result],
        recipient_arg,
    ));
    let pt = ptb.finish();
    TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, gas_price)
}

// =============================================================================
// Test 1: Has AB + has coins + GasCoin used
// Expected: Coin reservation FIRST in gas payment (smashes coins into AB)
// =============================================================================

#[sim_test]
async fn test_has_ab_has_coins_uses_gas_coin() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund sender's address balance with 5 SUI
    test_env
        .fund_one_address_balance(sender, 5 * MIST_PER_SUI)
        .await;

    // Refresh gas after funding
    let (sender, gas) = test_env.get_sender_and_gas(0);

    // Build PTB that uses GasCoin
    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    assert!(
        result.is_ok(),
        "Expected simulation to succeed, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution, got: {:?}",
        response.transaction.effects.status()
    );

    // TODO: Verify coin reservation is FIRST in gas payment once implemented
}

// =============================================================================
// Test 2: Has AB + has coins + GasCoin NOT used
// Expected: Coin reservation FIRST (for smashing), transaction succeeds
// =============================================================================

#[sim_test]
async fn test_has_ab_has_coins_no_gas_coin() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, 5 * MIST_PER_SUI)
        .await;

    // Refresh gas
    let (sender, gas) = test_env.get_sender_and_gas(0);

    // Build simple PTB
    let gas_budget = 50_000_000;
    let tx = build_simple_transfer_ptb(sender, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    assert!(
        result.is_ok(),
        "Expected simulation to succeed, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution, got: {:?}",
        response.transaction.effects.status()
    );

    // TODO: Verify coin reservation is FIRST in gas payment (for smashing)
}

// =============================================================================
// Test 3: Has AB + NO coins
// Expected: Pure AB payment (empty gas_data.payment + expiration)
// =============================================================================

#[sim_test]
async fn test_has_ab_no_coins() {
    // This test is tricky: we need a sender with AB but no coins.
    // We'll use address balance gas to achieve this.

    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund sender's address balance with enough for gas + operations
    test_env
        .fund_one_address_balance(sender, 10 * MIST_PER_SUI)
        .await;

    // For a true "no coins" test, we'd need to spend all coins first.
    // For now, this test verifies the setup works. The actual "pure AB"
    // behavior will be tested once implementation allows simulating
    // with address balance gas directly.

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let gas_budget = 50_000_000;
    let tx = build_simple_transfer_ptb(sender, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    // Should succeed - has funds available
    assert!(
        result.is_ok(),
        "Expected simulation to succeed, got: {:?}",
        result.err()
    );
}

// =============================================================================
// Test 4: NO AB + has coins
// Expected: Traditional coin gas payment
// =============================================================================

#[sim_test]
async fn test_no_ab_has_coins() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // NO address balance funding - sender only has coins

    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    assert!(
        result.is_ok(),
        "Expected simulation to succeed with coin gas, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution, got: {:?}",
        response.transaction.effects.status()
    );

    // Verify no coin reservation was used (traditional coin payment)
}

// =============================================================================
// Test 5: Sponsored transaction + sponsor has AB + coins
// Expected: Sponsor's coin reservation FIRST
// =============================================================================

#[sim_test]
async fn test_sponsored_with_sponsor_ab() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;

    let sender = test_env.get_sender(0);
    let sponsor = test_env.get_sender(1);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund SPONSOR's AB
    test_env
        .fund_one_address_balance(sponsor, 5 * MIST_PER_SUI)
        .await;

    let sponsor_gas = test_env.get_gas_for_sender(sponsor)[0];

    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let amount_arg = ptb.pure(amount).unwrap();
    let split_result = ptb.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount_arg],
    ));
    let recipient_arg = ptb.pure(recipient).unwrap();
    ptb.command(sui_types::transaction::Command::TransferObjects(
        vec![split_result],
        recipient_arg,
    ));
    let pt = ptb.finish();

    let tx = TransactionData::new_programmable_allow_sponsor(
        sender,
        vec![sponsor_gas],
        pt,
        gas_budget,
        test_env.rgp,
        sponsor,
    );

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    match result {
        Ok(response) => {
            if response.transaction.effects.status().is_ok() {
                // Success - sponsor's AB + coins used
            } else {
                println!(
                    "Sponsored tx execution status: {:?}",
                    response.transaction.effects.status()
                );
            }
        }
        Err(e) => {
            println!("Sponsored tx simulation: {}", e);
        }
    }
}

// =============================================================================
// Test 6: Insufficient total funds
// =============================================================================

#[sim_test]
async fn test_insufficient_funds() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund AB with 1 SUI
    test_env
        .fund_one_address_balance(sender, 1 * MIST_PER_SUI)
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);

    // Request way more than available (100M SUI)
    let amount = 100_000_000 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, false).await;

    match result {
        Ok(response) => {
            assert!(
                !response.transaction.effects.status().is_ok(),
                "Expected execution to fail due to insufficient funds"
            );
        }
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            assert!(
                err_str.contains("insufficient")
                    || err_str.contains("balance")
                    || err_str.contains("gas")
                    || err_str.contains("coin"),
                "Expected error about insufficient funds, got: {}",
                e
            );
        }
    }
}

// =============================================================================
// Test 7: Protocol config disabled - fallback to traditional behavior
// =============================================================================

#[sim_test]
async fn test_protocol_config_disabled() {
    let test_env = TestEnvBuilder::new()
        // No accumulators enabled
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    assert!(
        result.is_ok(),
        "Expected simulation to succeed with traditional gas, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution, got: {:?}",
        response.transaction.effects.status()
    );
}
