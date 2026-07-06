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
    coin_reservation::ParsedDigest,
    effects::TransactionEffectsAPI,
    gas_coin::{GAS, MIST_PER_SUI},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Argument, TransactionData, TransactionDataAPI},
};
use test_cluster::addr_balance_test_env::TestEnvBuilder;

/// Helper to build a PTB that splits X MIST from GasCoin and transfers to recipient.
/// When `gas` is provided, it's used as explicit gas payment.
/// When `gas` is None, gas selection will choose coins.
fn build_split_gas_coin_ptb(
    sender: SuiAddress,
    amount: u64,
    recipient: SuiAddress,
    gas: Option<sui_types::base_types::ObjectRef>,
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
    let gas_payment = gas.map(|g| vec![g]).unwrap_or_default();
    TransactionData::new_programmable(sender, gas_payment, pt, gas_budget, gas_price)
}

/// Helper to build a PTB that does NOT use Argument::GasCoin.
/// This is an empty PTB - it just pays gas without any operations.
/// When `gas` is provided, it's used as explicit gas payment.
/// When `gas` is None, gas selection will choose coins or AB.
fn build_no_gas_coin_ptb(
    sender: SuiAddress,
    gas: Option<sui_types::base_types::ObjectRef>,
    gas_budget: u64,
    gas_price: u64,
) -> TransactionData {
    let ptb = ProgrammableTransactionBuilder::new();
    let pt = ptb.finish();
    let gas_payment = gas.map(|g| vec![g]).unwrap_or_default();
    TransactionData::new_programmable(sender, gas_payment, pt, gas_budget, gas_price)
}

// =============================================================================
// Test 1: Has AB + has coins + GasCoin used
// Expected: Coin reservation FIRST in gas payment (smashes coins into AB)
// =============================================================================

#[sim_test]
async fn test_has_ab_has_coins_uses_gas_coin() {
    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund sender's address balance with 10 SUI - sufficient on its own
    let ab_amount = 10 * MIST_PER_SUI;
    test_env.fund_one_address_balance(sender, ab_amount).await;

    // Refresh sender after funding (gas not used - gas selection will choose)
    let (sender, _) = test_env.get_sender_and_gas(0);

    // Request 5 SUI - both AB (10 SUI) and coins (~30M SUI) can cover this independently
    let amount = 5 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    // Don't provide explicit gas coins - let gas selection choose them
    // This allows gas selection to prepend coin reservation
    let tx = build_split_gas_coin_ptb(sender, amount, recipient, None, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true, true).await;

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

    // Verify coin reservation is FIRST in gas payment (gas selection prepends it)
    let gas_payment = response.transaction.transaction.gas_data().payment.clone();
    assert!(
        !gas_payment.is_empty(),
        "Gas payment should not be empty when coins exist"
    );

    // First element should be a coin reservation (identified by magic in digest)
    let first_payment = &gas_payment[0];

    assert!(
        ParsedDigest::is_coin_reservation_digest(&first_payment.2),
        "First gas payment should be a coin reservation, got digest: {:?}",
        first_payment.2
    );

    // Verify the entire address balance is reserved, not just the gas budget
    // Note: The actual balance may be slightly less than ab_amount due to gas
    // consumed during the funding transaction
    let parsed_digest = ParsedDigest::try_from(first_payment.2)
        .expect("Should be able to parse coin reservation digest");
    let reservation_amount = parsed_digest.reservation_amount();
    assert!(
        reservation_amount >= ab_amount - 100_000_000, // Allow up to 0.1 SUI for gas
        "Coin reservation should reserve nearly the entire address balance, got {} (expected ~{})",
        reservation_amount,
        ab_amount
    );

    // Execute the simulated transaction to verify it's valid
    let simulated_tx = &response.transaction.transaction;
    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(simulated_tx)
        .await
        .expect("Simulated transaction should execute successfully");
    assert!(
        effects.status().is_ok(),
        "Executed transaction should succeed, got: {:?}",
        effects.status()
    );
}

// =============================================================================
// Test 2: Has AB + has coins + GasCoin NOT used
// Expected: Use AB if sufficient, otherwise use coins (no coin reservation needed)
// =============================================================================

#[sim_test]
async fn test_has_ab_has_coins_no_gas_coin() {
    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund sender's address balance - enough for a small gas budget but not a large one
    let ab_amount = MIST_PER_SUI;
    test_env.fund_one_address_balance(sender, ab_amount).await;

    // Refresh sender (gas not used - gas selection will choose)
    let (sender, _) = test_env.get_sender_and_gas(0);
    let client = test_env.cluster.grpc_client();

    // Case 1: Small budget that can be satisfied by AB alone
    // When GasCoin is not used, AB is preferred if sufficient
    // Don't provide explicit gas - let gas selection choose
    let small_budget = 50_000_000; // 0.05 SUI - easily covered by 1 SUI AB
    let tx_small = build_no_gas_coin_ptb(sender, None, small_budget, test_env.rgp);

    let result = client.simulate_transaction(&tx_small, true, true).await;
    assert!(
        result.is_ok(),
        "Expected simulation to succeed with AB alone, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution with AB alone, got: {:?}",
        response.transaction.effects.status()
    );

    // Verify no coin reservation was used (AB alone is sufficient, GasCoin not used)
    let gas_payment = response.transaction.transaction.gas_data().payment.clone();
    for (i, payment) in gas_payment.iter().enumerate() {
        assert!(
            !ParsedDigest::is_coin_reservation_digest(&payment.2),
            "Gas payment[{}] should NOT be a coin reservation when GasCoin not used",
            i
        );
    }

    // Execute to verify validity
    let simulated_tx = &response.transaction.transaction;
    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(simulated_tx)
        .await
        .expect("Simulated transaction should execute successfully");
    assert!(effects.status().is_ok());

    // Update gas objects after execution
    test_env.update_all_gas().await;

    // Case 2: Large budget that requires coins (AB alone insufficient)
    // Get fresh gas after first execution
    let (sender, _gas) = test_env.get_sender_and_gas(0);
    let large_budget = 5 * MIST_PER_SUI; // 5 SUI - exceeds 1 SUI AB
    // Don't provide explicit gas - let gas selection choose
    let tx_large = build_no_gas_coin_ptb(sender, None, large_budget, test_env.rgp);

    let result = client.simulate_transaction(&tx_large, true, true).await;
    assert!(
        result.is_ok(),
        "Expected simulation to succeed with coins, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution with coins, got: {:?}",
        response.transaction.effects.status()
    );

    // Verify no coin reservation (GasCoin not used, so no need to combine)
    let gas_payment = response.transaction.transaction.gas_data().payment.clone();
    for (i, payment) in gas_payment.iter().enumerate() {
        assert!(
            !ParsedDigest::is_coin_reservation_digest(&payment.2),
            "Gas payment[{}] should NOT be a coin reservation when GasCoin not used",
            i
        );
    }

    // Execute to verify validity
    let simulated_tx = &response.transaction.transaction;
    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(simulated_tx)
        .await
        .expect("Simulated transaction should execute successfully");
    assert!(effects.status().is_ok());
}

// =============================================================================
// Test 3: Has AB + NO coins
// Expected: Pure AB payment (empty gas_data.payment + expiration)
// =============================================================================

#[sim_test]
async fn test_has_ab_no_coins() {
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::transaction::TransactionExpiration;

    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;

    // Use an existing genesis account (sender1) so its AB is known to the fullnode reader.
    // Transfer all of sender1's coins to sender2, so sender1 only has AB.
    let (sender1, _) = test_env.get_sender_and_gas(0);
    let (sender2, _) = test_env.get_sender_and_gas(1);
    let ab_amount = 10 * MIST_PER_SUI;

    // Fund sender1's address balance
    test_env.fund_one_address_balance(sender1, ab_amount).await;

    // Get all coins for sender1 and transfer them to sender2
    let (_, all_gas) = test_env.get_sender_and_all_gas(0);
    let mut transfer_digests = Vec::new();
    for coin in all_gas {
        let transfer_tx = TestTransactionBuilder::new(sender1, coin, test_env.rgp)
            .transfer_sui(None, sender2) // Transfer entire coin
            .build();
        let tx = test_env.cluster.wallet.sign_transaction(&transfer_tx).await;
        let executed = test_env.cluster.execute_transaction(tx).await;
        assert!(executed.effects.status().is_ok(), "Transfer should succeed");
        transfer_digests.push(*executed.effects.transaction_digest());
    }
    // Wait for all transfers to settle in the RPC service
    test_env
        .cluster
        .wait_for_tx_settlement(&transfer_digests)
        .await;

    // Refresh gas objects
    test_env.update_all_gas().await;

    // Verify sender1 has no coins
    let sender1_coins = test_env
        .gas_objects
        .get(&sender1)
        .cloned()
        .unwrap_or_default();
    assert!(
        sender1_coins.is_empty(),
        "sender1 should have no coins, has: {:?}",
        sender1_coins
    );

    // Verify sender1 has AB
    let ab_balance = test_env.get_sui_balance_ab(sender1);
    assert!(
        ab_balance > 0,
        "sender1 should have address balance, got: {}",
        ab_balance
    );

    // Now sender1 has:
    // - Coins: NONE (all transferred to sender2)
    // - AB: 10 SUI
    //
    // Build a transaction that does NOT use GasCoin (empty PTB)
    // This should use pure AB payment with ValidDuring expiration

    // Build transaction with empty gas payment - the simulate API should perform
    // gas selection and discover we can use pure AB payment
    let gas_budget = 50_000_000; // 0.05 SUI
    let ptb = ProgrammableTransactionBuilder::new();
    let pt = ptb.finish();
    let tx = TransactionData::new_programmable(sender1, vec![], pt, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true, true).await;

    assert!(
        result.is_ok(),
        "Expected simulation to succeed with pure AB payment, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution with pure AB payment, got: {:?}",
        response.transaction.effects.status()
    );

    // Verify pure AB payment: gas_data.payment should be empty
    let gas_payment = response.transaction.transaction.gas_data().payment.clone();
    assert!(
        gas_payment.is_empty(),
        "Gas payment should be empty for pure AB payment, got: {:?}",
        gas_payment
    );

    // Verify expiration is set (ValidDuring for pure AB payment)
    let expiration = response.transaction.transaction.expiration();
    assert!(
        matches!(expiration, TransactionExpiration::ValidDuring { .. }),
        "Expected ValidDuring expiration for pure AB payment, got: {:?}",
        expiration
    );

    // Execute the simulated transaction to verify it's valid
    let simulated_tx = &response.transaction.transaction;
    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(simulated_tx)
        .await
        .expect("Simulated transaction should execute successfully");
    assert!(
        effects.status().is_ok(),
        "Executed transaction should succeed, got: {:?}",
        effects.status()
    );
}

// =============================================================================
// Test 4: NO AB + has coins
// Expected: Traditional coin gas payment
// =============================================================================

#[sim_test]
async fn test_no_ab_has_coins() {
    let test_env = TestEnvBuilder::new().build().await;

    let (sender, _gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // NO address balance funding - sender only has coins

    let amount = MIST_PER_SUI;
    let gas_budget = 50_000_000;

    // Don't provide explicit gas - let gas selection choose
    let tx = build_split_gas_coin_ptb(sender, amount, recipient, None, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true, true).await;

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
    let gas_payment = response.transaction.transaction.gas_data().payment.clone();
    assert!(!gas_payment.is_empty(), "Gas payment should not be empty");

    // No element should be a coin reservation
    for (i, payment) in gas_payment.iter().enumerate() {
        assert!(
            !ParsedDigest::is_coin_reservation_digest(&payment.2),
            "Gas payment[{}] should NOT be a coin reservation when no AB exists",
            i
        );
    }

    // Execute the simulated transaction to verify it's valid
    let simulated_tx = &response.transaction.transaction;
    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(simulated_tx)
        .await
        .expect("Simulated transaction should execute successfully");
    assert!(
        effects.status().is_ok(),
        "Executed transaction should succeed, got: {:?}",
        effects.status()
    );
}

// =============================================================================
// Test 5: Insufficient total funds
// =============================================================================

#[sim_test]
async fn test_insufficient_funds() {
    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    // Fund AB with 1 SUI
    test_env
        .fund_one_address_balance(sender, MIST_PER_SUI)
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);

    // Request way more than available (100M SUI)
    let amount = 100_000_000 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    // Use explicit gas coins - this test uses do_gas_selection=false
    let tx = build_split_gas_coin_ptb(
        sender,
        amount,
        recipient,
        Some(gas),
        gas_budget,
        test_env.rgp,
    );

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, false, false).await;

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

    let (sender, _gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();

    let amount = MIST_PER_SUI;
    let gas_budget = 50_000_000;

    // Don't provide explicit gas - let gas selection choose
    let tx = build_split_gas_coin_ptb(sender, amount, recipient, None, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true, true).await;

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

    // Execute the simulated transaction to verify it's valid
    let simulated_tx = &response.transaction.transaction;
    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(simulated_tx)
        .await
        .expect("Simulated transaction should execute successfully");
    assert!(
        effects.status().is_ok(),
        "Executed transaction should succeed, got: {:?}",
        effects.status()
    );
}

// =============================================================================
// Test 8: Combined AB + coins when neither alone is sufficient
// Expected: Compat layer combines both sources via coin reservation
// =============================================================================

#[sim_test]
async fn test_combined_ab_and_coins_needed() {
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::crypto::{AccountKeyPair, SuiKeyPair, get_key_pair};

    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;

    // Create a new account with limited funds to demonstrate the combined scenario
    let (limited_sender, keypair): (SuiAddress, AccountKeyPair) = get_key_pair();
    let keypair = SuiKeyPair::Ed25519(keypair);
    let recipient = SuiAddress::random_for_testing_only();

    // Add the new account to the wallet so we can sign transactions
    test_env.cluster.wallet.add_account(None, keypair).await;

    // Use genesis account to transfer 10 SUI coin to limited_sender
    let (genesis_sender, genesis_gas) = test_env.get_sender_and_gas(0);
    let coin_amount = 10 * MIST_PER_SUI;
    let transfer_tx = TestTransactionBuilder::new(genesis_sender, genesis_gas, test_env.rgp)
        .transfer_sui(Some(coin_amount), limited_sender)
        .build();
    let (_, effects) = test_env
        .exec_tx_directly(transfer_tx)
        .await
        .expect("Transfer should succeed");
    assert!(effects.status().is_ok());

    // Find the coin that was created for limited_sender
    // (gas selection will find it - we don't provide explicit gas)
    let _created_coin = effects
        .created()
        .iter()
        .find(|(_, owner)| {
            matches!(owner, sui_types::object::Owner::AddressOwner(addr) if *addr == limited_sender)
        })
        .map(|(obj_ref, _)| *obj_ref)
        .expect("Should have created a coin for limited_sender");

    // Fund limited_sender's address balance with 5 SUI (from genesis account)
    let genesis_gas = test_env.get_gas_for_sender(genesis_sender)[0];
    let ab_amount = 5 * MIST_PER_SUI;
    let fund_ab_tx = TestTransactionBuilder::new(genesis_sender, genesis_gas, test_env.rgp)
        .transfer_sui_to_address_balance(
            sui_test_transaction_builder::FundSource::coin(genesis_gas),
            vec![(ab_amount, limited_sender)],
        )
        .build();
    let (digest, effects) = test_env
        .exec_tx_directly(fund_ab_tx)
        .await
        .expect("Fund AB should succeed");
    assert!(effects.status().is_ok());
    // Wait for the settlement transactions to commit so that the accumulator
    // reflects limited_sender's AB before gas selection queries it.
    test_env.cluster.wait_for_tx_settlement(&[digest]).await;

    // Now limited_sender has:
    // - Coin: 10 SUI
    // - AB: 5 SUI
    // - Total: 15 SUI
    //
    // Request 12 SUI - this requires BOTH sources:
    // - Coin alone (10 SUI) is insufficient
    // - AB alone (5 SUI) is insufficient
    // - Combined (15 SUI) is sufficient
    let request_amount = 12 * MIST_PER_SUI;
    let gas_budget = 50_000_000;

    // Don't provide explicit gas - let gas selection combine AB + coins
    let tx = build_split_gas_coin_ptb(
        limited_sender,
        request_amount,
        recipient,
        None,
        gas_budget,
        test_env.rgp,
    );

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true, true).await;

    assert!(
        result.is_ok(),
        "Expected simulation to succeed with combined AB + coins, got: {:?}",
        result.err()
    );

    let response = result.unwrap();

    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution with combined funds, got: {:?}",
        response.transaction.effects.status()
    );

    // Verify coin reservation is used to combine both sources
    let gas_payment = response.transaction.transaction.gas_data().payment.clone();
    assert!(!gas_payment.is_empty(), "Gas payment should not be empty");

    // First element should be a coin reservation (AB contribution)
    let first_payment = &gas_payment[0];
    assert!(
        ParsedDigest::is_coin_reservation_digest(&first_payment.2),
        "First gas payment should be a coin reservation when combining AB + coins"
    );
}

// =============================================================================
// Test: AB-only gas payment succeeds when budget > half of address balance
// Regression test for double-counting of gas budget in select_gas.
// =============================================================================

#[sim_test]
async fn test_ab_only_budget_exceeds_half_balance() {
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::transaction::TransactionExpiration;

    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;

    let (sender1, _) = test_env.get_sender_and_gas(0);
    let (sender2, _) = test_env.get_sender_and_gas(1);

    // Fund sender1's address balance with 1 SUI.
    let ab_amount = MIST_PER_SUI;
    test_env.fund_one_address_balance(sender1, ab_amount).await;

    // Transfer all of sender1's coins to sender2 so sender1 only has AB.
    let (_, all_gas) = test_env.get_sender_and_all_gas(0);
    let mut transfer_digests = Vec::new();
    for coin in all_gas {
        let transfer_tx = TestTransactionBuilder::new(sender1, coin, test_env.rgp)
            .transfer_sui(None, sender2)
            .build();
        let tx = test_env.cluster.wallet.sign_transaction(&transfer_tx).await;
        let executed = test_env.cluster.execute_transaction(tx).await;
        assert!(executed.effects.status().is_ok(), "Transfer should succeed");
        transfer_digests.push(*executed.effects.transaction_digest());
    }
    test_env
        .cluster
        .wait_for_tx_settlement(&transfer_digests)
        .await;
    test_env.update_all_gas().await;

    // Verify sender1 has no coins but does have AB.
    let sender1_coins = test_env
        .gas_objects
        .get(&sender1)
        .cloned()
        .unwrap_or_default();
    assert!(sender1_coins.is_empty(), "sender1 should have no coins");

    let ab_balance = test_env.get_sui_balance_ab(sender1);
    assert!(ab_balance > 0, "sender1 should have address balance");

    // Budget is more than half the AB but less than the full AB. Before the fix,
    // select_gas would double-count the budget when computing available balance,
    // requiring raw_balance >= 2 * budget instead of raw_balance >= budget.
    let gas_budget = (ab_balance * 3) / 4; // 75% of AB
    assert!(gas_budget > ab_balance / 2);
    assert!(gas_budget <= ab_balance);

    let ptb = ProgrammableTransactionBuilder::new();
    let pt = ptb.finish();
    let tx = TransactionData::new_programmable(sender1, vec![], pt, gas_budget, test_env.rgp);

    let client = test_env.cluster.grpc_client();
    let result = client.simulate_transaction(&tx, true, true).await;

    assert!(
        result.is_ok(),
        "Simulation should succeed when AB covers the budget, got: {:?}",
        result.err()
    );

    let response = result.unwrap();
    assert!(
        response.transaction.effects.status().is_ok(),
        "Expected successful execution with pure AB payment, got: {:?}",
        response.transaction.effects.status()
    );

    // Verify pure AB payment path was taken.
    let gas_payment = response.transaction.transaction.gas_data().payment.clone();
    assert!(
        gas_payment.is_empty(),
        "Gas payment should be empty for pure AB payment, got: {:?}",
        gas_payment
    );

    assert!(
        matches!(
            response.transaction.transaction.expiration(),
            TransactionExpiration::ValidDuring { .. }
        ),
        "Expected ValidDuring expiration for pure AB payment, got: {:?}",
        response.transaction.transaction.expiration()
    );

    // Execute the simulated transaction to verify it's actually valid.
    let simulated_tx = &response.transaction.transaction;
    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(simulated_tx)
        .await
        .expect("Simulated transaction should execute successfully");
    assert!(
        effects.status().is_ok(),
        "Executed transaction should succeed, got: {:?}",
        effects.status()
    );
}

/// Convert an internal ObjectRef to a proto ObjectReference with all fields set.
fn object_ref_to_proto(
    obj_ref: &sui_types::base_types::ObjectRef,
) -> sui_rpc::proto::sui::rpc::v2::ObjectReference {
    let mut message = sui_rpc::proto::sui::rpc::v2::ObjectReference::default();
    message.object_id = Some(obj_ref.0.to_hex_uncompressed());
    message.version = Some(obj_ref.1.value());
    message.digest = Some(obj_ref.2.to_string());
    message
}

// =============================================================================
// Test 9: Resolve handles coin reservation ObjectRefs in gas payment
// Expected: The resolve path recognizes coin reservation ObjectRefs and passes
// them through without trying to look them up as regular gas coins.
// =============================================================================

#[sim_test]
async fn test_resolve_handles_coin_reservation_in_gas_payment() {
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_rpc::proto::sui::rpc::v2::{
        Argument, Command, GasPayment, Input, MoveCall, ObjectReference, ProgrammableTransaction,
        SimulateTransactionRequest, Transaction, TransactionKind,
    };

    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund sender's address balance with 10 SUI.
    let ab_amount = 10 * MIST_PER_SUI;
    test_env.fund_one_address_balance(sender, ab_amount).await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let gas_budget = 50_000_000;

    // Skip if mainnet override disabled coin reservation.
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    // Look up the actual address balance and construct a coin reservation
    // directly rather than relying on a prior simulate to produce one.
    let balance = test_env.get_sui_balance_ab(sender);
    assert!(balance > 0, "sender should have address balance");
    let coin_reservation = test_env.encode_coin_reservation(sender, 0, balance);

    // Build a proto-format transaction with the coin reservation in gas
    // payment alongside a regular gas coin. Using an unresolved clock input
    // (object_id only, no version/digest) forces the resolve path.
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_env.cluster.rpc_url().to_owned())
            .await
            .unwrap();

    let gas_objects: Vec<ObjectReference> = vec![
        object_ref_to_proto(&coin_reservation),
        object_ref_to_proto(&gas),
    ];

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![{
            let mut message = Input::default();
            message.object_id = Some("0x6".to_owned());
            message
        }];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x2".to_owned());
            message.module = Some("clock".to_owned());
            message.function = Some("timestamp_ms".to_owned());
            message.arguments = vec![Argument::new_input(0)];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());
    unresolved_transaction.gas_payment = Some({
        let mut gp = GasPayment::default();
        gp.owner = Some(sender.to_string());
        gp.objects = gas_objects;
        gp.budget = Some(gas_budget);
        gp.price = Some(test_env.rgp);
        gp
    });

    // Simulate through the resolve path. Before the fix, this fails because
    // resolve_gas_object_reference tries to look up the masked coin
    // reservation ObjectID as a regular object.
    let result = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await;

    assert!(
        result.is_ok(),
        "Resolve should handle coin reservation ObjectRefs in gas payment: {:?}",
        result.err()
    );

    let response = result.unwrap().into_inner();
    assert!(
        response.transaction().effects().status().success(),
        "Resolved transaction with coin reservation should simulate successfully"
    );
}

// =============================================================================
// Test 10: Resolve handles coin reservation ObjectRefs as PTB inputs
// Expected: The resolve path recognizes coin reservation ObjectRefs used as
// ImmutableOrOwned inputs and passes them through without a storage lookup.
// =============================================================================

#[sim_test]
async fn test_resolve_handles_coin_reservation_in_ptb_input() {
    use sui_rpc::proto::sui::rpc::v2::input::InputKind;
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_rpc::proto::sui::rpc::v2::{
        Argument, Command, Input, MoveCall, ProgrammableTransaction, SimulateTransactionRequest,
        Transaction, TransactionKind,
    };

    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;
    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund sender's address balance with 10 SUI.
    let ab_amount = 10 * MIST_PER_SUI;
    test_env.fund_one_address_balance(sender, ab_amount).await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    // Skip if mainnet override disabled coin reservation.
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    // Look up the actual address balance and construct a coin reservation
    // directly rather than relying on a prior simulate to produce one.
    let balance = test_env.get_sui_balance_ab(sender);
    assert!(balance > 0, "sender should have address balance");
    let coin_reservation = test_env.encode_coin_reservation(sender, 0, balance);

    // Build a proto-format transaction that includes the coin reservation
    // ObjectRef as a PTB input (ImmutableOrOwned). Using an unresolved clock
    // input alongside it forces the resolve path.
    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_env.cluster.rpc_url().to_owned())
            .await
            .unwrap();

    let coin_res_input = {
        let mut message = Input::default();
        message.set_kind(InputKind::ImmutableOrOwned);
        message.object_id = Some(coin_reservation.0.to_hex_uncompressed());
        message.version = Some(coin_reservation.1.value());
        message.digest = Some(coin_reservation.2.to_string());
        message
    };

    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![
            // Input 0: coin reservation ObjectRef as ImmutableOrOwned.
            coin_res_input,
        ];
        ptb.commands = vec![Command::from({
            let mut message = MoveCall::default();
            message.package = Some("0x2".to_owned());
            message.module = Some("coin".to_owned());
            message.function = Some("send_funds".to_owned());
            message.arguments = vec![Argument::new_input(0)];
            message.type_arguments = vec![GAS::type_().to_string()];
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());

    // Simulate through the resolve path. Before the fix, this fails because
    // resolve_object_reference tries to look up the masked coin reservation
    // ObjectID as a regular object.
    let result = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await;

    assert!(
        result.is_ok(),
        "Resolve should handle coin reservation ObjectRefs as PTB inputs: {:?}",
        result.err()
    );
}

// =============================================================================
// Regression: when gas selection picks address balance, the estimated budget
// must not include the storage cost of the synthetic gas coin used internally
// by the simulator. Real execution charges gas via an accumulator event with
// no gas-coin write, so any phantom storage cost in the budget is over-billing.
// =============================================================================

#[sim_test]
async fn test_estimated_budget_excludes_mock_gas_coin_storage_for_address_balance() {
    use shared_crypto::intent::Intent;
    use sui_keys::keystore::AccountKeystore;
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_rpc::proto::sui::rpc::v2::{
        Argument, Command, GasPayment, Input, ProgrammableTransaction, SimulateTransactionRequest,
        Transaction, TransactionKind, TransferObjects,
    };

    let test_env = TestEnvBuilder::new().build().await;

    let mut test_env = test_env;
    let recipient = SuiAddress::random_for_testing_only();

    // Sender owns a coin that the PTB transfers; the PTB never uses Argument::GasCoin so
    // gas selection is free to use sponsor's address balance.
    let (sender, sender_coin) = test_env.get_sender_and_gas(0);
    let (sponsor, _) = test_env.get_sender_and_gas(1);

    // Fund the sponsor's address balance so that gas selection picks address balance.
    let sponsor_ab_amount = 5 * MIST_PER_SUI;
    test_env
        .fund_one_address_balance(sponsor, sponsor_ab_amount)
        .await;

    // Build an unresolved Transaction proto. Critical that the request omits both BCS and
    // a gas budget — that's the only configuration that exercises the budget-estimation
    // path that this fix targets.
    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![
            {
                let mut input = Input::default();
                input.object_id = Some(sender_coin.0.to_canonical_string(true));
                input
            },
            {
                let mut input = Input::default();
                input.literal = Some(Box::new(recipient.to_string().into()));
                input
            },
        ];
        ptb.commands = vec![Command::from({
            let mut message = TransferObjects::default();
            message.objects = vec![Argument::new_input(0)];
            message.address = Some(Argument::new_input(1));
            message
        })];
        ptb
    }));
    unresolved_transaction.sender = Some(sender.to_string());
    unresolved_transaction.gas_payment = Some({
        let mut message = GasPayment::default();
        message.owner = Some(sponsor.to_string());
        message
    });

    let mut alpha_client =
        TransactionExecutionServiceClient::connect(test_env.cluster.rpc_url().to_owned())
            .await
            .unwrap();

    let response = alpha_client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let resolved_transaction: TransactionData = response
        .transaction
        .as_ref()
        .unwrap()
        .transaction
        .as_ref()
        .unwrap()
        .bcs
        .as_ref()
        .unwrap()
        .deserialize()
        .unwrap();

    assert!(
        resolved_transaction.gas_data().payment.is_empty(),
        "Expected gas selection to pick address balance, got payment: {:?}",
        resolved_transaction.gas_data().payment
    );

    let simulated_budget = resolved_transaction.gas_data().budget;

    // Sign with both sender and sponsor and execute the resolved transaction so we can
    // measure what the gas actually costs at execution time.
    let sender_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &resolved_transaction, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &resolved_transaction, Intent::sui_transaction())
        .await
        .unwrap();
    let signed = sui_types::transaction::Transaction::from_data(
        resolved_transaction,
        vec![sender_sig, sponsor_sig],
    );
    let executed = test_env.cluster.execute_transaction(signed).await;
    assert!(
        executed.effects.status().is_ok(),
        "Executed transaction should succeed, got: {:?}",
        executed.effects.status()
    );

    let summary = executed.effects.gas_cost_summary();
    let actual_net_gas = summary.net_gas_usage();

    let overshoot = simulated_budget as i64 - actual_net_gas;
    assert!(
        overshoot >= 0,
        "Simulated budget {simulated_budget} must cover actual net gas usage {actual_net_gas}",
    );

    // The estimator adds a `1000 * reference_gas_price` safe-overhead buffer (defined in
    // `estimate_gas_budget_from_gas_cost`). Without this fix, the synthetic gas coin's
    // storage write — `object_size * obj_data_cost_refundable * storage_gas_price` MIST,
    // typically ~1M MIST under default protocol params for a Coin<SUI> object — also
    // lands in the budget. Bound the overshoot at `1500 * RGP` so the safe-overhead
    // alone fits comfortably while a leaked storage cost trips this assertion.
    let max_expected_overshoot = 1_500u64.saturating_mul(test_env.rgp);
    assert!(
        (overshoot as u64) < max_expected_overshoot,
        "Estimated budget overshot actual net gas usage by {overshoot} MIST \
         (simulated={simulated_budget}, actual={actual_net_gas}); \
         max expected overshoot is {max_expected_overshoot} MIST. \
         The mock gas coin's storage cost is leaking into the estimate."
    );
}
