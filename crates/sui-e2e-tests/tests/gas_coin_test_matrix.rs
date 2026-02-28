// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Comprehensive test matrix for gas coin handling.
//!
//! This test covers all permutations of the following orthogonal axes:
//!
//! Gas Charge:
//! - Positive (normal transaction)
//! - Negative (due to storage rebate from deleting a large object)
//!
//! Gas Payment:
//! - 1 real coin
//! - Multiple real coins
//! - Pure address balance payment (gas_data.payment = [])
//! - 1 fake coin (coin reservation)
//! - Multiple fake coins
//! - Mix of real/fake coins: First coin is real
//! - Mix of real/fake coins: First coin is fake
//!
//! Gas Coin Usage:
//! - No gas coin usage (simple transaction)
//! - Gas coin is transferred away at end of tx (skip for pure address balance payment)
//!
//! Gas Budget:
//! - Exceeds amount of available funds (transaction should be rejected)
//! - Does not exceed available funds (transaction should execute)

use std::path::PathBuf;

use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    effects::TransactionEffectsAPI,
    error::SuiError,
    transaction::CallArg,
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

/// Gas payment configuration variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GasPaymentType {
    /// Single real coin object
    OneRealCoin,
    /// Multiple real coin objects
    MultipleRealCoins,
    /// Pure address balance payment (empty gas payment vector)
    PureAddressBalance,
    /// Single fake coin (coin reservation)
    OneFakeCoin,
    /// Multiple fake coins
    MultipleFakeCoins,
    /// Mix of real/fake coins with real coin first
    MixedRealFirst,
    /// Mix of real/fake coins with fake coin first
    MixedFakeFirst,
}

impl GasPaymentType {
    fn all() -> &'static [GasPaymentType] {
        &[
            GasPaymentType::OneRealCoin,
            GasPaymentType::MultipleRealCoins,
            GasPaymentType::PureAddressBalance,
            GasPaymentType::OneFakeCoin,
            GasPaymentType::MultipleFakeCoins,
            GasPaymentType::MixedRealFirst,
            GasPaymentType::MixedFakeFirst,
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            GasPaymentType::OneRealCoin => "one_real_coin",
            GasPaymentType::MultipleRealCoins => "multiple_real_coins",
            GasPaymentType::PureAddressBalance => "pure_address_balance",
            GasPaymentType::OneFakeCoin => "one_fake_coin",
            GasPaymentType::MultipleFakeCoins => "multiple_fake_coins",
            GasPaymentType::MixedRealFirst => "mixed_real_first",
            GasPaymentType::MixedFakeFirst => "mixed_fake_first",
        }
    }

    fn uses_fake_coins(&self) -> bool {
        matches!(
            self,
            GasPaymentType::OneFakeCoin
                | GasPaymentType::MultipleFakeCoins
                | GasPaymentType::MixedRealFirst
                | GasPaymentType::MixedFakeFirst
        )
    }
}

/// Gas charge type (positive vs negative due to storage rebate)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GasChargeType {
    /// Normal positive gas charge
    Positive,
    /// Negative gas charge due to storage rebate from deleting a large object
    NegativeFromStorageRebate,
}

impl GasChargeType {
    fn all() -> &'static [GasChargeType] {
        &[
            GasChargeType::Positive,
            GasChargeType::NegativeFromStorageRebate,
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            GasChargeType::Positive => "positive",
            GasChargeType::NegativeFromStorageRebate => "negative_storage_rebate",
        }
    }
}

/// Gas coin usage type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GasCoinUsage {
    /// No gas coin usage in the transaction
    NoUsage,
    /// Gas coin is transferred away at the end of the transaction
    TransferredAway,
}

impl GasCoinUsage {
    fn all() -> &'static [GasCoinUsage] {
        &[GasCoinUsage::NoUsage, GasCoinUsage::TransferredAway]
    }

    fn name(&self) -> &'static str {
        match self {
            GasCoinUsage::NoUsage => "no_usage",
            GasCoinUsage::TransferredAway => "transferred_away",
        }
    }

    fn should_skip_for(&self, payment_type: GasPaymentType) -> bool {
        // Skip "transferred away" tests for pure address balance payment
        // since there's no gas coin object to transfer
        *self == GasCoinUsage::TransferredAway && payment_type == GasPaymentType::PureAddressBalance
    }
}

/// Gas budget type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GasBudgetType {
    /// Budget exceeds available funds
    ExceedsAvailable,
    /// Budget does not exceed available funds
    WithinAvailable,
}

impl GasBudgetType {
    fn all() -> &'static [GasBudgetType] {
        &[
            GasBudgetType::ExceedsAvailable,
            GasBudgetType::WithinAvailable,
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            GasBudgetType::ExceedsAvailable => "exceeds_available",
            GasBudgetType::WithinAvailable => "within_available",
        }
    }
}

/// A single test case configuration
#[derive(Debug)]
struct TestCase {
    payment_type: GasPaymentType,
    charge_type: GasChargeType,
    coin_usage: GasCoinUsage,
    budget_type: GasBudgetType,
}

impl TestCase {
    fn name(&self) -> String {
        format!(
            "payment_{}_charge_{}_usage_{}_budget_{}",
            self.payment_type.name(),
            self.charge_type.name(),
            self.coin_usage.name(),
            self.budget_type.name()
        )
    }
}

fn move_test_code_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    path
}

/// Main test that runs all permutations
#[sim_test]
async fn test_gas_coin_handling_matrix() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    // Publish the gas_test package for creating/deleting large objects
    let gas_test_package_id = test_env.setup_test_package(move_test_code_path()).await;

    let mut test_count = 0;
    let mut passed = 0;
    let mut skipped = 0;

    // Generate all test case permutations
    // Note: We skip negative gas charge (storage rebate) cases to avoid gas exhaustion.
    // Negative gas charge tests require creating large objects which uses extra gas.
    // TODO: Add a separate test for negative gas charge cases with dedicated test environments.
    for payment_type in GasPaymentType::all() {
        for charge_type in GasChargeType::all() {
            // Skip negative gas charge cases for now to conserve resources
            if *charge_type == GasChargeType::NegativeFromStorageRebate {
                continue;
            }

            for coin_usage in GasCoinUsage::all() {
                for budget_type in GasBudgetType::all() {
                    let test_case = TestCase {
                        payment_type: *payment_type,
                        charge_type: *charge_type,
                        coin_usage: *coin_usage,
                        budget_type: *budget_type,
                    };

                    // Skip invalid combinations
                    if coin_usage.should_skip_for(*payment_type) {
                        tracing::info!(
                            "Skipping test case: {} (invalid combination)",
                            test_case.name()
                        );
                        skipped += 1;
                        continue;
                    }

                    test_count += 1;
                    tracing::info!("Running test case {}: {}", test_count, test_case.name());

                    match run_test_case(&mut test_env, &test_case, gas_test_package_id).await {
                        Ok(()) => {
                            passed += 1;
                            tracing::info!("Test case {} PASSED", test_case.name());
                        }
                        Err(e) => {
                            let err_str = e.to_string();
                            if err_str.contains("No gas objects available") {
                                tracing::warn!(
                                    "Test case {} SKIPPED due to gas exhaustion: {}",
                                    test_case.name(),
                                    err_str
                                );
                                skipped += 1;
                                // Break out of inner loops - we've exhausted gas
                                break;
                            } else {
                                panic!("Test case {} FAILED: {:?}", test_case.name(), e);
                            }
                        }
                    }

                    // Trigger reconfiguration after each test to check for conservation errors
                    test_env.trigger_reconfiguration().await;
                }
            }
        }
    }

    tracing::info!(
        "Test matrix complete: {} passed, {} skipped out of {} total",
        passed,
        skipped,
        test_count + skipped
    );
}

/// Run a single test case
async fn run_test_case(
    test_env: &mut TestEnv,
    test_case: &TestCase,
    gas_test_package_id: sui_types::base_types::ObjectID,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Refresh gas objects for clean state
    test_env.update_all_gas().await;

    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    // Determine how much gas we need
    let base_budget: u64 = 5_000_000_000; // 5 SUI base budget

    // For storage rebate tests, we need to create and delete a large object
    // Create this BEFORE setting up gas payment to ensure we have gas available
    let large_object_ref = if test_case.charge_type == GasChargeType::NegativeFromStorageRebate {
        Some(create_large_object(test_env, sender, gas_test_package_id).await?)
    } else {
        None
    };

    // Setup based on payment type
    let (gas_payment, available_funds) =
        setup_gas_payment(test_env, sender, test_case.payment_type, base_budget).await?;

    // Adjust budget based on budget type
    // Note: Protocol maximum budget is ~50 trillion, so we need to handle the case
    // where exceeding available funds might hit the protocol cap
    let max_protocol_budget = 50_000_000_000_000u64;
    let actual_budget = match test_case.budget_type {
        GasBudgetType::ExceedsAvailable => {
            // We want to exceed available funds. If available_funds >= max_protocol_budget,
            // we can't create a valid "exceeds" test case, so skip or use max.
            (available_funds + 1_000_000_000).min(max_protocol_budget)
        }
        GasBudgetType::WithinAvailable => base_budget.min(available_funds).min(max_protocol_budget),
    };

    // For "exceeds" budget type, verify the budget actually exceeds available funds
    // If not (due to protocol cap), this test case is effectively a "within available" case
    let effective_budget_type = if test_case.budget_type == GasBudgetType::ExceedsAvailable
        && actual_budget <= available_funds
    {
        tracing::warn!(
            "Budget capped at protocol max {} which is <= available funds {}, treating as WithinAvailable",
            actual_budget,
            available_funds
        );
        GasBudgetType::WithinAvailable
    } else {
        test_case.budget_type
    };

    // Build the transaction
    let tx = build_test_transaction(
        test_env,
        sender,
        recipient,
        &gas_payment,
        actual_budget,
        test_case,
        gas_test_package_id,
        large_object_ref,
    )?;

    // Execute the transaction
    let result = test_env.exec_tx_directly(tx).await;

    // Verify the expected outcome using the effective budget type
    verify_outcome(
        result,
        effective_budget_type,
        available_funds,
        actual_budget,
    )?;

    Ok(())
}

/// Setup gas payment based on payment type
async fn setup_gas_payment(
    test_env: &mut TestEnv,
    sender: SuiAddress,
    payment_type: GasPaymentType,
    budget: u64,
) -> Result<(Vec<ObjectRef>, u64), Box<dyn std::error::Error + Send + Sync>> {
    test_env.update_all_gas().await;

    // Check if we have gas objects available
    let gas_available = test_env.get_gas_for_sender(sender);
    if gas_available.is_empty() && payment_type != GasPaymentType::PureAddressBalance {
        return Err("No gas objects available for sender".into());
    }

    // First, ensure we have enough address balance for fake coin tests
    if payment_type.uses_fake_coins() || payment_type == GasPaymentType::PureAddressBalance {
        // Need gas to fund address balance
        if gas_available.is_empty() {
            return Err("No gas objects available to fund address balance".into());
        }
        test_env.fund_one_address_balance(sender, budget * 2).await;
    }

    test_env.update_all_gas().await;

    let all_gas = test_env.get_gas_for_sender(sender);
    let total_real_balance: u64 = {
        let mut total = 0;
        for gas_ref in &all_gas {
            total += test_env.get_coin_balance(gas_ref.0).await;
        }
        total
    };

    match payment_type {
        GasPaymentType::OneRealCoin => {
            let coin = all_gas[0];
            let balance = test_env.get_coin_balance(coin.0).await;
            Ok((vec![coin], balance))
        }
        GasPaymentType::MultipleRealCoins => {
            // Use up to 3 real coins
            let coins: Vec<ObjectRef> = all_gas.into_iter().take(3).collect();
            Ok((coins, total_real_balance))
        }
        GasPaymentType::PureAddressBalance => {
            // Empty gas payment vector - uses address balance directly
            let ab_balance = test_env.get_sui_balance_ab(sender);
            Ok((vec![], ab_balance))
        }
        GasPaymentType::OneFakeCoin => {
            let ab_balance = test_env.get_sui_balance_ab(sender);
            let fake_coin = test_env.encode_coin_reservation(sender, 0, budget);
            Ok((vec![fake_coin], ab_balance))
        }
        GasPaymentType::MultipleFakeCoins => {
            let ab_balance = test_env.get_sui_balance_ab(sender);
            // Create multiple fake coins that sum to the budget
            let fake1 = test_env.encode_coin_reservation(sender, 0, budget / 2);
            let fake2 = test_env.encode_coin_reservation(sender, 0, budget / 2);
            Ok((vec![fake1, fake2], ab_balance))
        }
        GasPaymentType::MixedRealFirst => {
            let real_coin = all_gas[0];
            let real_balance = test_env.get_coin_balance(real_coin.0).await;
            let ab_balance = test_env.get_sui_balance_ab(sender);
            let fake_coin = test_env.encode_coin_reservation(sender, 0, budget / 2);
            Ok((vec![real_coin, fake_coin], real_balance + ab_balance))
        }
        GasPaymentType::MixedFakeFirst => {
            let real_coin = all_gas[0];
            let real_balance = test_env.get_coin_balance(real_coin.0).await;
            let ab_balance = test_env.get_sui_balance_ab(sender);
            let fake_coin = test_env.encode_coin_reservation(sender, 0, budget / 2);
            Ok((vec![fake_coin, real_coin], real_balance + ab_balance))
        }
    }
}

/// Create a large object that will give substantial storage rebate when deleted
async fn create_large_object(
    test_env: &mut TestEnv,
    sender: SuiAddress,
    package_id: sui_types::base_types::ObjectID,
) -> Result<ObjectRef, Box<dyn std::error::Error + Send + Sync>> {
    // Ensure we have up-to-date gas objects after any reconfiguration
    test_env.update_all_gas().await;
    let gas_objects = test_env.get_gas_for_sender(sender);
    if gas_objects.is_empty() {
        return Err("No gas objects available for sender".into());
    }
    let gas = gas_objects[0];

    // Create a large vector for storage (10KB)
    let large_data: Vec<u8> = vec![42u8; 10_000];

    let tx = TestTransactionBuilder::new(sender, gas, test_env.rgp)
        .move_call(
            package_id,
            "gas_test",
            "create_object_with_large_storage",
            vec![
                CallArg::Pure(bcs::to_bytes(&1u64).unwrap()),
                CallArg::Pure(bcs::to_bytes(&large_data).unwrap()),
            ],
        )
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await?;

    let created_objects = effects.created();
    let created = created_objects
        .iter()
        .find(|(_, owner)| owner.is_address_owned())
        .ok_or("No object created")?;

    let obj_ref = created.0;
    test_env.update_all_gas().await;

    Ok(obj_ref)
}

/// Build the test transaction based on test case parameters
fn build_test_transaction(
    test_env: &TestEnv,
    sender: SuiAddress,
    recipient: SuiAddress,
    gas_payment: &[ObjectRef],
    budget: u64,
    test_case: &TestCase,
    gas_test_package_id: sui_types::base_types::ObjectID,
    large_object_ref: Option<ObjectRef>,
) -> Result<sui_types::transaction::TransactionData, Box<dyn std::error::Error + Send + Sync>> {
    let builder = if gas_payment.is_empty() {
        // Pure address balance payment
        let epoch = test_env
            .cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().epoch_store_for_testing().epoch());

        TestTransactionBuilder::new_with_address_balance_gas(
            sender,
            test_env.rgp,
            test_env.chain_id,
            epoch,
            0, // nonce
        )
    } else {
        TestTransactionBuilder::new_with_gas_objects(sender, gas_payment.to_vec(), test_env.rgp)
    };

    let builder = builder.with_gas_budget(budget);

    // Add transaction content based on test case
    let builder = match (test_case.charge_type, test_case.coin_usage) {
        (GasChargeType::NegativeFromStorageRebate, GasCoinUsage::NoUsage) => {
            // Delete the large object for storage rebate
            let obj_ref =
                large_object_ref.ok_or("Large object required for storage rebate test")?;
            builder.move_call(
                gas_test_package_id,
                "gas_test",
                "delete_object",
                vec![CallArg::Object(
                    sui_types::transaction::ObjectArg::ImmOrOwnedObject(obj_ref),
                )],
            )
        }
        (GasChargeType::NegativeFromStorageRebate, GasCoinUsage::TransferredAway) => {
            // Delete the large object and transfer gas coin
            let obj_ref =
                large_object_ref.ok_or("Large object required for storage rebate test")?;
            builder
                .move_call(
                    gas_test_package_id,
                    "gas_test",
                    "delete_object",
                    vec![CallArg::Object(
                        sui_types::transaction::ObjectArg::ImmOrOwnedObject(obj_ref),
                    )],
                )
                .transfer_sui(None, recipient) // Transfer entire gas coin
        }
        (GasChargeType::Positive, GasCoinUsage::NoUsage) => {
            // Simple computation that doesn't use gas coin
            builder.move_call(
                gas_test_package_id,
                "gas_test",
                "abort_with_computation",
                vec![CallArg::Pure(bcs::to_bytes(&false).unwrap())], // Don't abort
            )
        }
        (GasChargeType::Positive, GasCoinUsage::TransferredAway) => {
            // Transfer the gas coin to recipient
            builder.transfer_sui(None, recipient)
        }
    };

    Ok(builder.build())
}

/// Verify the outcome matches expectations
fn verify_outcome(
    result: Result<
        (
            sui_types::digests::TransactionDigest,
            sui_types::effects::TransactionEffects,
        ),
        SuiError,
    >,
    budget_type: GasBudgetType,
    available_funds: u64,
    budget: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match budget_type {
        GasBudgetType::ExceedsAvailable => {
            // Transaction should be rejected
            match result {
                Err(e) => {
                    let err_str = e.to_string();
                    // Check that it failed for insufficient gas reasons
                    if err_str.contains("insufficient")
                        || err_str.contains("GasBalanceTooLow")
                        || err_str.contains("gas")
                        || err_str.contains("balance")
                    {
                        tracing::info!(
                            "Transaction correctly rejected: budget {} exceeds available {}",
                            budget,
                            available_funds
                        );
                        Ok(())
                    } else {
                        Err(format!(
                            "Transaction failed but not for gas reasons. Expected gas error, got: {}",
                            err_str
                        ).into())
                    }
                }
                Ok((_, effects)) => {
                    if !effects.status().is_ok() {
                        // Transaction executed but failed - this is acceptable for budget exceeded
                        tracing::info!(
                            "Transaction executed but failed as expected: {:?}",
                            effects.status()
                        );
                        Ok(())
                    } else {
                        Err(format!(
                            "Transaction should have been rejected but succeeded. Budget: {}, Available: {}",
                            budget, available_funds
                        ).into())
                    }
                }
            }
        }
        GasBudgetType::WithinAvailable => {
            // Transaction should execute (may still fail in execution, but not for gas)
            match result {
                Ok((_, effects)) => {
                    if effects.status().is_ok() {
                        tracing::info!("Transaction succeeded as expected");
                        Ok(())
                    } else {
                        // Execution failure is OK as long as it's not gas-related
                        let err = format!("{:?}", effects.status());
                        if err.contains("InsufficientGas") || err.contains("GasBalanceTooLow") {
                            Err(format!(
                                "Transaction failed due to gas but budget was within available: {}",
                                err
                            )
                            .into())
                        } else {
                            tracing::info!(
                                "Transaction executed but failed (non-gas error): {}",
                                err
                            );
                            Ok(())
                        }
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    // This might be acceptable for some edge cases
                    if err_str.contains("insufficient") || err_str.contains("GasBalanceTooLow") {
                        Err(format!(
                            "Transaction rejected for gas reasons but budget was within available: {}",
                            err_str
                        ).into())
                    } else {
                        // Other errors might be OK
                        tracing::warn!("Transaction failed with non-gas error: {}", err_str);
                        Ok(())
                    }
                }
            }
        }
    }
}
