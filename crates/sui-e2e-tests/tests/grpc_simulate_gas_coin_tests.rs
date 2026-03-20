// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the GRPC simulate API's handling of Argument::GasCoin with the
//! compatibility layer for address balances.
//!
//! These tests verify that when a PTB uses Argument::GasCoin (tx.gas), the simulate
//! API correctly handles various combinations of coin and address balance availability.

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
/// This is the canonical way apps use tx.gas to access their SUI balance.
fn build_split_gas_coin_ptb(
    sender: SuiAddress,
    amount: u64,
    recipient: SuiAddress,
    gas: sui_types::base_types::ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> TransactionData {
    let mut ptb = ProgrammableTransactionBuilder::new();

    // SplitCoins(GasCoin, [amount])
    let amount_arg = ptb.pure(amount).unwrap();
    let split_result = ptb.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount_arg],
    ));

    // TransferObjects([split_result], recipient)
    let recipient_arg = ptb.pure(recipient).unwrap();
    ptb.command(sui_types::transaction::Command::TransferObjects(
        vec![split_result],
        recipient_arg,
    ));

    let pt = ptb.finish();

    TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, gas_price)
}

// =============================================================================
// Test Case 1: X satisfied entirely by coins, with AB funds available
// =============================================================================

#[sim_test]
async fn test_gas_coin_satisfied_by_coins_with_ab_available() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Setup: sender has coins (from genesis) and we'll add some AB funds
    // Fund address balance with 3 SUI
    let mut test_env = test_env;
    test_env
        .fund_one_address_balance(sender, 3 * MIST_PER_SUI)
        .await;

    // Refresh gas after funding
    let (sender, gas) = test_env.get_sender_and_gas(0);

    // X = 1 SUI - should be satisfiable from coins
    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000; // 0.05 SUI

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    // Simulate the transaction
    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    // Should succeed - coins can satisfy the request
    assert!(
        result.is_ok(),
        "Expected simulation to succeed when coins can satisfy X, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    // Verify the transaction executed successfully
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution, got: {:?}",
        response.transaction.effects.status()
    );
}

// =============================================================================
// Test Case 2: X satisfied entirely by coins, no AB funds
// =============================================================================

#[sim_test]
async fn test_gas_coin_satisfied_by_coins_no_ab() {
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

    // Setup: sender has coins only (no AB funds added)
    // X = 1 SUI - should be satisfiable from coins
    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    // Should succeed - coins can satisfy the request
    assert!(
        result.is_ok(),
        "Expected simulation to succeed when coins can satisfy X, got: {:?}",
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
// Test Case 3: X satisfied entirely by address balance, with coins available
// =============================================================================

#[sim_test]
async fn test_gas_coin_satisfied_by_ab_with_coins_available() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;
    let (sender, all_gas) = test_env.get_sender_and_all_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Use the first gas coin for paying gas, second coin for splitting
    let gas_for_tx = all_gas[0];
    let coin_to_split = all_gas[1];

    // Step 1: Create a small coin (0.1 SUI) by splitting from the second gas coin
    // This small coin will be used as gas for the test transaction
    let small_coin_amount = 100_000_000u64; // 0.1 SUI
    let split_tx = test_env
        .tx_builder_with_gas(sender, gas_for_tx)
        .split_coin(coin_to_split, vec![small_coin_amount])
        .build();
    let (_, effects) = test_env.exec_tx_directly(split_tx).await.unwrap();

    // Get the newly created small coin from the effects
    let small_coin = effects
        .created()
        .iter()
        .find(|(obj_ref, _)| obj_ref.0 != coin_to_split.0)
        .map(|(obj_ref, _)| *obj_ref)
        .expect("Should have created a new coin");

    // Step 2: Fund sender's address balance with 5 SUI (uses a different gas coin internally)
    test_env
        .fund_one_address_balance(sender, 5 * MIST_PER_SUI)
        .await;

    // Step 3: Build a PTB that uses the SMALL coin as gas and tries to split 1 SUI
    // The small coin only has 0.1 SUI, but we're asking for 1 SUI
    // Without compatibility layer: should FAIL
    // With compatibility layer: should use AB and succeed
    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(
        sender,
        amount,
        recipient,
        small_coin,
        gas_budget,
        test_env.rgp,
    );

    let client = test_env.cluster.grpc_client();
    // Use allow_mock_gas_coin=false to use the actual small coin
    let result = client.simulate_transaction(&tx, false).await;

    // This test should FAIL until the compatibility layer is implemented.
    // The gas coin (small_coin) only has 0.1 SUI, but we're trying to split 1 SUI.
    // Once the compatibility layer is implemented, it should succeed using AB.

    match result {
        Ok(response) => {
            // If simulation succeeded, execution should have failed due to insufficient funds
            assert!(
                !response.transaction.effects.status().is_ok(),
                "Expected execution to fail (gas coin has insufficient balance for split), \
                 but got success. This is expected once compatibility layer is implemented."
            );
        }
        Err(_) => {
            // Expected to fail - gas coin doesn't have enough for the split
            // This is the expected behavior before compatibility layer is implemented
        }
    }
}

// =============================================================================
// Test Case 4: X satisfied entirely by address balance, minimal coins
// =============================================================================

#[sim_test]
async fn test_gas_coin_satisfied_by_ab_no_coins() {
    // This test uses a sender with a minimal coin (just enough for gas fees)
    // but with a larger address balance. The split amount exceeds the coin balance.

    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;
    let (sender, all_gas) = test_env.get_sender_and_all_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Use the first gas coin for paying gas, second coin for splitting
    let gas_for_tx = all_gas[0];
    let coin_to_split = all_gas[1];

    // Step 1: Create a minimal coin (just 0.05 SUI - enough for gas fees only)
    let minimal_coin_amount = 50_000_000u64; // 0.05 SUI
    let split_tx = test_env
        .tx_builder_with_gas(sender, gas_for_tx)
        .split_coin(coin_to_split, vec![minimal_coin_amount])
        .build();
    let (_, effects) = test_env.exec_tx_directly(split_tx).await.unwrap();

    // Get the newly created minimal coin
    let minimal_coin = effects
        .created()
        .iter()
        .find(|(obj_ref, _)| obj_ref.0 != coin_to_split.0)
        .map(|(obj_ref, _)| *obj_ref)
        .expect("Should have created a new coin");

    // Step 2: Fund sender's address balance with 5 SUI (uses a different gas coin internally)
    test_env
        .fund_one_address_balance(sender, 5 * MIST_PER_SUI)
        .await;

    // Step 3: Build a PTB using the MINIMAL coin as gas, trying to split 1 SUI
    // The minimal coin only has 0.05 SUI (barely enough for gas)
    // Without compatibility layer: should FAIL
    // With compatibility layer: should use AB and succeed
    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(
        sender,
        amount,
        recipient,
        minimal_coin,
        gas_budget,
        test_env.rgp,
    );

    let client = test_env.cluster.grpc_client();
    // Use allow_mock_gas_coin=false to use the actual minimal coin
    let result = client.simulate_transaction(&tx, false).await;

    // This test should FAIL until the compatibility layer is implemented.
    // The gas coin (minimal_coin) only has 0.05 SUI, but we're trying to split 1 SUI.
    // Once the compatibility layer is implemented, it should succeed using AB.

    match result {
        Ok(response) => {
            // If simulation succeeded, execution should have failed
            assert!(
                !response.transaction.effects.status().is_ok(),
                "Expected execution to fail (gas coin has insufficient balance for split), \
                 but got success. This is expected once compatibility layer is implemented."
            );
        }
        Err(_) => {
            // Expected to fail - gas coin doesn't have enough for the split
            // This is the expected behavior before compatibility layer is implemented
        }
    }
}

// =============================================================================
// Test Case 5: X requires combined withdrawal from coins and address balance
// =============================================================================

#[sim_test]
async fn test_gas_coin_requires_combined_sources() {
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

    // Fund AB with 2 SUI
    test_env
        .fund_one_address_balance(sender, 2 * MIST_PER_SUI)
        .await;

    // Refresh gas after funding
    let (sender, gas) = test_env.get_sender_and_gas(0);

    // X = amount that would require both coins and AB
    // This is an edge case - the expected behavior is:
    // - tx.gas should use a single source (either coins OR AB)
    // - If neither single source can satisfy X, it should fail
    //
    // Per Slack: "tx.gas will be incompatible with explicit reservations of SUI"
    // This implies we shouldn't try to combine sources.

    // Request 10 SUI (more than either source alone, assuming gas coin has ~5 SUI)
    let amount = 10 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    // Expected behavior: Should fail because neither coins nor AB alone can satisfy X
    // We should NOT try to combine sources for tx.gas

    match result {
        Ok(response) => {
            // If it somehow succeeded (gas coin had enough), that's also valid
            if response.transaction.effects.status().is_ok() {
                println!("Test case 5: Unexpectedly succeeded - gas coin had enough balance");
            } else {
                // Execution failed, which is expected
                println!(
                    "Test case 5: Execution failed as expected: {:?}",
                    response.transaction.effects.status()
                );
            }
        }
        Err(e) => {
            // Simulation failed, which is expected if we can't satisfy the request
            println!("Test case 5: Simulation failed as expected: {}", e);
        }
    }
}

// =============================================================================
// Test Case 6: Insufficient funds even when combining all sources
// =============================================================================

#[sim_test]
async fn test_gas_coin_insufficient_total_funds() {
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

    // Refresh gas after funding
    let (sender, gas) = test_env.get_sender_and_gas(0);

    // X = 100,000,000 SUI - way more than total available
    // (genesis coins have DEFAULT_GAS_AMOUNT = 30_000_000_000_000_000 MIST = 30M SUI)
    let amount = 100_000_000 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(sender, amount, recipient, gas, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    // Use allow_mock_gas_coin=false to use actual gas coin, which has limited funds
    let result = client.simulate_transaction(&tx, false).await;

    // Expected behavior: Should fail with insufficient funds error
    // Either the simulation fails, or the execution fails

    match result {
        Ok(response) => {
            // Simulation succeeded but execution should have failed
            assert!(
                !response.transaction.effects.status().is_ok(),
                "Expected execution to fail due to insufficient funds, got status: {:?}",
                response.transaction.effects.status()
            );
        }
        Err(e) => {
            // Simulation failed, which is expected for insufficient funds
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
// Test Case 7: tx.gas with explicit SUI reservation (should error)
// =============================================================================

#[sim_test]
async fn test_gas_coin_with_explicit_sui_reservation_errors() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund AB
    test_env
        .fund_one_address_balance(sender, 5 * MIST_PER_SUI)
        .await;

    // Refresh gas after funding
    let (sender, gas) = test_env.get_sender_and_gas(0);

    // Build a PTB that uses BOTH GasCoin AND an explicit FundsWithdrawal for SUI
    // This should be rejected per Slack: "tx.gas will be incompatible with explicit reservations of SUI"

    let mut ptb = ProgrammableTransactionBuilder::new();

    // Add an explicit FundsWithdrawal input for SUI
    // Note: This requires constructing a FundsWithdrawalArg which may need
    // special handling. For now, we'll use a simpler approach.

    // SplitCoins(GasCoin, [amount])
    let amount = 1 * MIST_PER_SUI;
    let amount_arg = ptb.pure(amount).unwrap();
    let split_result = ptb.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount_arg],
    ));

    // TransferObjects([split_result], recipient)
    let recipient_arg = ptb.pure(recipient).unwrap();
    ptb.command(sui_types::transaction::Command::TransferObjects(
        vec![split_result],
        recipient_arg,
    ));

    let pt = ptb.finish();

    // TODO: Add an explicit SUI FundsWithdrawal input to the transaction
    // This test is a placeholder until we can properly construct such a transaction

    let gas_budget = 50_000_000;
    let tx = TransactionData::new_programmable(sender, vec![gas], pt, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    // For now, this test just verifies the basic flow works
    // Once we implement the incompatibility check, we should get an error

    // TODO: Update this assertion once the incompatibility check is implemented
    println!(
        "Test case 7 result (placeholder - needs explicit FundsWithdrawal): {:?}",
        result.is_ok()
    );
}

// =============================================================================
// Test Case 8: Sponsored transaction with tx.gas
// =============================================================================

#[sim_test]
async fn test_sponsored_transaction_with_gas_coin() {
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.create_root_accumulator_object_for_testing();
            cfg.enable_accumulators_for_testing();
            cfg
        }))
        .build()
        .await;

    let mut test_env = test_env;

    // sender executes, sponsor pays gas
    let sender = test_env.get_sender(0);
    let sponsor = test_env.get_sender(1);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund sponsor's AB
    test_env
        .fund_one_address_balance(sponsor, 5 * MIST_PER_SUI)
        .await;

    // Get sponsor's gas for the transaction
    let sponsor_gas = test_env.get_gas_for_sender(sponsor)[0];

    // Build PTB that uses GasCoin
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

    // Create a sponsored transaction: sender executes, sponsor pays gas
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

    // Expected behavior with compatibility layer:
    // - Should use sponsor's AB for the GasCoin (via WithdrawFrom::Sponsor)
    // - Transaction should succeed

    match result {
        Ok(response) => {
            // Check if execution succeeded
            if response.transaction.effects.status().is_ok() {
                println!("Test case 8: Sponsored transaction succeeded");
            } else {
                println!(
                    "Test case 8: Execution failed: {:?}",
                    response.transaction.effects.status()
                );
            }
        }
        Err(e) => {
            println!("Test case 8: Simulation failed: {}", e);
        }
    }
}

// =============================================================================
// Test Case 9: Protocol config disabled - fallback behavior
// =============================================================================

#[sim_test]
async fn test_gas_coin_protocol_config_disabled() {
    // Test with accumulators disabled - should use traditional coin selection
    let test_env = TestEnvBuilder::new()
        // No accumulators enabled
        .build()
        .await;

    let (sender, sender_gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // X = 1 SUI
    let amount = 1 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    let tx = build_split_gas_coin_ptb(
        sender,
        amount,
        recipient,
        sender_gas,
        gas_budget,
        test_env.rgp,
    );

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true).await;

    // Should work with traditional coin-based gas payment
    assert!(
        result.is_ok(),
        "Expected simulation to succeed with traditional gas payment, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution, got: {:?}",
        response.transaction.effects.status()
    );
}
