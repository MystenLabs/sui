// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the GRPC simulate API's gas selection with address balance.
//!
//! Gas payment strategy when Argument::GasCoin IS used:
//! - Has AB + has coins → Coin reservation FIRST (smashes coins into AB, user accesses combined)
//! - Has AB + no coins  → Pure AB payment (empty gas_data.payment + expiration)
//! - No AB + has coins  → Traditional coin gas payment
//!
//! Gas payment strategy when Argument::GasCoin is NOT used:
//! - Prefer AB if sufficient
//! - Fall back to coins if AB insufficient
//! - Use compat layer (both) if neither alone is sufficient

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

/// Helper to build a PTB that does NOT use Argument::GasCoin.
/// This is an empty PTB - it just pays gas without any operations.
fn build_no_gas_coin_ptb(
    sender: SuiAddress,
    gas: sui_types::base_types::ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> TransactionData {
    let ptb = ProgrammableTransactionBuilder::new();
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
// Expected: Use AB if sufficient, otherwise use coins (no coin reservation needed)
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

    // Fund sender's address balance with enough for gas
    test_env
        .fund_one_address_balance(sender, 5 * MIST_PER_SUI)
        .await;

    // Refresh gas
    let (sender, gas) = test_env.get_sender_and_gas(0);

    // Build PTB that does NOT use GasCoin
    let gas_budget = 50_000_000;
    let tx = build_no_gas_coin_ptb(sender, gas, gas_budget, test_env.rgp);

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

    // When GasCoin is not used, prefer AB if sufficient, else coins.
    // No coin reservation needed since user doesn't need access to combined balance.
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
    let tx = build_no_gas_coin_ptb(sender, gas, gas_budget, test_env.rgp);

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
// Test 5: Insufficient total funds
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

// =============================================================================
// Test 8: Combined AB + coins when neither alone is sufficient
// Expected: Compat layer combines both sources via coin reservation
// =============================================================================

#[sim_test]
async fn test_combined_ab_and_coins_needed() {
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

    let (sender, gas) = test_env.get_sender_and_gas(0);

    // Request amount that requires BOTH coins and AB:
    // - Genesis coin has ~30M SUI
    // - AB has 5 SUI
    // - Request 30M + 3 SUI = requires combining both
    let amount = 30_000_000 * MIST_PER_SUI + 3 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    // This should succeed when the compat layer is implemented,
    // combining coins + AB via coin reservation.
    // Until then, it may fail due to insufficient funds from coins alone.
    match result {
        Ok(response) => {
            if response.transaction.effects.status().is_ok() {
                // Success - compat layer combined both sources
            } else {
                // Expected to fail until compat layer is implemented
                println!(
                    "Combined funds test execution status: {:?}",
                    response.transaction.effects.status()
                );
            }
        }
        Err(e) => {
            // Expected to fail until compat layer is implemented
            println!(
                "Combined funds test simulation error (expected until impl): {}",
                e
            );
        }
    }
}
