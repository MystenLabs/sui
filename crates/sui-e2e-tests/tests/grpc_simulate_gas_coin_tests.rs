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
    base_types::{ObjectRef, SuiAddress},
    coin_reservation::ParsedDigest,
    crypto::AccountKeyPair,
    effects::TransactionEffectsAPI,
    gas_coin::{GAS, MIST_PER_SUI},
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        Argument, GasData, ObjectArg, SharedObjectMutability, Transaction as SignedTransaction,
        TransactionData, TransactionDataAPI, TransactionDataV1, TransactionExpiration,
    },
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

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

    // Verify an epoch-scoped expiration is set for pure AB payment. Simulate returns `Validity`
    // when it can also name the transaction's proposers, and `ValidDuring` otherwise.
    let expiration = response.transaction.transaction.expiration();
    assert!(
        matches!(
            expiration,
            TransactionExpiration::ValidDuring { .. } | TransactionExpiration::Validity { .. }
        ),
        "Expected ValidDuring or Validity expiration for pure AB payment, got: {:?}",
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
    use sui_types::transaction::TransactionExpiration;

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

    // Even a coin-paid transaction gets a proposer restriction when the fullnode can name its
    // preferred validators; otherwise the expiration stays untouched. When a set is present it
    // must be well-formed, or the transaction we execute below would be rejected outright.
    match response.transaction.transaction.expiration() {
        TransactionExpiration::None => (),
        TransactionExpiration::Validity {
            allowed_proposers: Some(allowed),
            ..
        } => {
            assert!(allowed.proposers.iter().is_sorted_by(|a, b| a < b));
            assert!(allowed.proposers.len() <= 3);
        }
        other => panic!("unexpected expiration for coin-paid transaction: {other:?}"),
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
// Test 4b: Clients that don't report a protocol version never get allowed proposers,
// since they may not be able to decode `TransactionExpiration::Validity`.
// =============================================================================

#[sim_test]
async fn test_no_allowed_proposers_without_client_protocol_version() {
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_rpc::proto::sui::rpc::v2::{Bcs, SimulateTransactionRequest, Transaction};
    use sui_types::transaction::TransactionExpiration;

    let test_env = TestEnvBuilder::new().build().await;

    let (sender, _gas) = test_env.get_sender_and_gas(0);
    let recipient = SuiAddress::random_for_testing_only();
    let tx = build_split_gas_coin_ptb(
        sender,
        MIST_PER_SUI,
        recipient,
        None,
        50_000_000,
        test_env.rgp,
    );

    // A bare tonic client, which sends no `x-sui-client-protocol-version` header.
    let mut client =
        TransactionExecutionServiceClient::connect(test_env.cluster.rpc_url().to_owned())
            .await
            .unwrap();
    let response = client
        .simulate_transaction(
            SimulateTransactionRequest::new(
                Transaction::default().with_bcs(Bcs::serialize(&tx).unwrap()),
            )
            .with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();

    let resolved: TransactionData = response
        .transaction()
        .transaction()
        .bcs()
        .deserialize()
        .unwrap();
    assert_eq!(resolved.expiration(), &TransactionExpiration::None);
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
            TransactionExpiration::ValidDuring { .. } | TransactionExpiration::Validity { .. }
        ),
        "Expected ValidDuring or Validity expiration for pure AB payment, got: {:?}",
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

// =============================================================================
// Regression: with gas selection off, a transaction that pays for gas from
// address balance must be simulated as it executes, without the synthetic gas
// coin. The estimated budget excludes that coin's storage cost, so at mainnet's
// reference gas price charging it fails the simulation with InsufficientGas
// even though execution succeeds.
// =============================================================================

/// Mainnet's reference gas price in MIST. At this price the estimated budget's `1000 * RGP` safe
/// overhead is smaller than the storage cost of a synthetic gas coin.
const MAINNET_REFERENCE_GAS_PRICE: u64 = 100;

/// Simulate `tx` with checks on and gas selection off, then execute it. Both must succeed and
/// charge the same gas.
async fn assert_simulation_matches_execution(test_env: &TestEnv, tx: &TransactionData) {
    let simulated = test_env
        .cluster
        .grpc_client()
        .simulate_transaction(tx, true, false)
        .await
        .expect("Simulation request should succeed");
    assert!(
        simulated.transaction.effects.status().is_ok(),
        "Simulation should succeed, got: {:?}",
        simulated.transaction.effects.status()
    );

    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(tx)
        .await
        .expect("Transaction should execute");
    assert!(
        effects.status().is_ok(),
        "Execution should succeed, got: {:?}",
        effects.status()
    );
    assert_eq!(
        simulated.transaction.effects.gas_cost_summary(),
        effects.gas_cost_summary(),
        "Simulation should charge the same gas as execution"
    );
}

#[sim_test]
async fn test_simulate_address_balance_gas_without_gas_selection() {
    use rand::rngs::OsRng;
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_rpc::proto::sui::rpc::v2::{
        Argument, Command, GasPayment, Input, ProgrammableTransaction, SimulateTransactionRequest,
        Transaction, TransactionKind, TransferObjects,
    };
    use sui_swarm_config::genesis_config::ValidatorGenesisConfigBuilder;
    use sui_types::transaction::ObjectArg;

    let mut test_env = TestEnvBuilder::new()
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder.with_validators(vec![
                ValidatorGenesisConfigBuilder::new()
                    .with_gas_price(MAINNET_REFERENCE_GAS_PRICE)
                    .build(&mut OsRng),
            ])
        }))
        .build()
        .await;
    assert_eq!(test_env.rgp, MAINNET_REFERENCE_GAS_PRICE);

    let recipient = SuiAddress::random_for_testing_only();
    let (sender, _) = test_env.get_sender_and_gas(0);
    test_env
        .fund_one_address_balance(sender, MIST_PER_SUI / 2)
        .await;
    let (_, coins) = test_env.get_sender_and_all_gas(0);

    // Case 1: resolve the budget and address-balance gas with gas selection, then simulate the
    // resolved transaction, which carries a ValidDuring expiration, without it.
    let mut unresolved_transaction = Transaction::default();
    unresolved_transaction.kind = Some(TransactionKind::from({
        let mut ptb = ProgrammableTransaction::default();
        ptb.inputs = vec![
            {
                let mut input = Input::default();
                input.object_id = Some(coins[0].0.to_canonical_string(true));
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
        message.owner = Some(sender.to_string());
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
    assert_simulation_matches_execution(&test_env, &resolved_transaction).await;

    // Case 2: the same kind of transfer without an expiration. The owned coin input protects it
    // against replay, so it still pays for gas from address balance. The budget from case 1 does
    // not carry over: this genesis coin has no storage rebate to offset its write.
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coins[1])).unwrap();
    builder.transfer_arg(recipient, coin_arg);
    let tx = TransactionData::new_programmable(
        sender,
        vec![],
        builder.finish(),
        5_000_000,
        test_env.rgp,
    );
    assert_simulation_matches_execution(&test_env, &tx).await;
}

// =============================================================================
// With gas selection off, a transaction without gas payment that could not pay
// for gas from address balance as given is still simulated with the synthetic
// gas coin, so dry runs of unpaid transactions keep working.
// =============================================================================

#[sim_test]
async fn test_simulate_without_gas_selection_falls_back_to_mock_gas_coin() {
    use sui_types::transaction::ObjectArg;

    let mut test_env = TestEnvBuilder::new().build().await;
    let client = test_env.cluster.grpc_client();
    let gas_budget = 50_000_000;

    // An owned input protects the transaction against replay, but the sender has no address
    // balance to pay for gas.
    let (sender, coin) = test_env.get_sender_and_gas(0);
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap();
    builder.transfer_arg(SuiAddress::random_for_testing_only(), coin_arg);
    let tx = TransactionData::new_programmable(
        sender,
        vec![],
        builder.finish(),
        gas_budget,
        test_env.rgp,
    );
    let response = client
        .simulate_transaction(&tx, true, false)
        .await
        .expect("Simulation without address balance should use a mock gas coin");
    assert!(
        response.transaction.effects.status().is_ok(),
        "Simulation without address balance should succeed, got: {:?}",
        response.transaction.effects.status()
    );

    // The address balance covers the budget, but without a gas coin nothing protects the
    // transaction against replay: it has no inputs and no expiration.
    let (sender, _) = test_env.get_sender_and_gas(1);
    test_env
        .fund_one_address_balance(sender, MIST_PER_SUI)
        .await;
    let tx = build_no_gas_coin_ptb(sender, None, gas_budget, test_env.rgp);
    let response = client
        .simulate_transaction(&tx, true, false)
        .await
        .expect("Simulation without replay protection should use a mock gas coin");
    assert!(
        response.transaction.effects.status().is_ok(),
        "Simulation without replay protection should succeed, got: {:?}",
        response.transaction.effects.status()
    );
}

async fn issue_allowance(
    env: &mut TestEnv,
    spender: SuiAddress,
    amount: u64,
) -> (
    sui_types::base_types::ObjectID,
    sui_types::base_types::SequenceNumber,
) {
    use move_core_types::identifier::Identifier;
    use sui_types::{SUI_FRAMEWORK_PACKAGE_ID, object::Owner};

    let funder = env.get_sender(0);
    let mut pt = ProgrammableTransactionBuilder::new();
    let amount = pt.pure(amount).unwrap();
    let payment = pt.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        vec![amount],
    ));
    let spender = pt.pure(spender).unwrap();
    pt.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gas_allowance").unwrap(),
        Identifier::new("new").unwrap(),
        vec![],
        vec![payment, spender],
    );
    let (_, effects) = env
        .exec_tx_directly(TransactionData::new_programmable(
            funder,
            vec![env.get_gas_for_sender(funder)[0]],
            pt.finish(),
            20_000_000,
            env.rgp,
        ))
        .await
        .unwrap();
    effects
        .created()
        .into_iter()
        .find_map(|(object, owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => Some((object.0, initial_shared_version)),
            _ => None,
        })
        .unwrap()
}

async fn create_protected_gas_coin(
    env: &mut TestEnv,
    spender: SuiAddress,
    key: &AccountKeyPair,
    allowance: (
        sui_types::base_types::ObjectID,
        sui_types::base_types::SequenceNumber,
    ),
    amount: u64,
    nonce: u32,
) -> (SuiAddress, ObjectRef) {
    let funder = env.get_sender(0);
    let funding = env
        .tx_builder_with_gas(funder, env.get_gas_for_sender(funder)[0])
        .transfer_sui(Some(amount), spender)
        .build();
    let (_, funding_effects) = env.exec_tx_directly(funding).await.unwrap();
    assert!(funding_effects.status().is_ok());
    let source_coin = funding_effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == Owner::AddressOwner(spender))
        .expect("funding transfer should create the source coin")
        .0;
    let mut pt = ProgrammableTransactionBuilder::new();
    let source_coin = pt.obj(ObjectArg::ImmOrOwnedObject(source_coin)).unwrap();
    let amount = pt.pure(amount).unwrap();
    let coin = pt.command(sui_types::transaction::Command::SplitCoins(
        source_coin,
        vec![amount],
    ));
    pt.programmable_move_call(
        sui_types::SUI_FRAMEWORK_PACKAGE_ID,
        move_core_types::identifier::Identifier::new("coin").unwrap(),
        move_core_types::identifier::Identifier::new("destroy_zero").unwrap(),
        vec![sui_types::gas_coin::GAS::type_tag()],
        vec![source_coin],
    );
    let recipient = pt.pure(spender).unwrap();
    pt.command(sui_types::transaction::Command::TransferObjects(
        vec![coin],
        recipient,
    ));
    pt.obj(ObjectArg::SharedObject {
        id: allowance.0,
        initial_shared_version: allowance.1,
        mutability: SharedObjectMutability::Mutable,
    })
    .unwrap();
    let allowance_paid = TransactionData::V1(TransactionDataV1 {
        kind: sui_types::transaction::TransactionKind::ProgrammableTransaction(pt.finish()),
        sender: spender,
        gas_data: GasData {
            payment: vec![],
            owner: allowance.0.into(),
            price: env.rgp,
            budget: 20_000_000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: env.chain_id,
            nonce,
        },
    });
    let (allowance_digest, allowance_effects) = env
        .cluster
        .execute_transaction_directly(&SignedTransaction::from_data_and_signer(
            allowance_paid,
            vec![key],
        ))
        .await
        .unwrap();
    env.cluster
        .wait_for_tx_settlement(&[allowance_digest])
        .await;
    assert!(allowance_effects.status().is_ok());

    let mut protected_gas_coin = None;
    for (object_ref, owner) in allowance_effects.created() {
        if owner != Owner::AddressOwner(spender) {
            continue;
        }
        let object = env
            .cluster
            .get_object_from_fullnode_store(&object_ref.0)
            .await
            .expect("created gas coin should be available");
        if sui_types::gas_coin::GasCoin::try_from(&object).is_ok() {
            protected_gas_coin = Some(object_ref);
            break;
        }
    }
    let protected_gas_coin = protected_gas_coin.expect("allowance gas creates a protected coin");
    (funder, protected_gas_coin)
}

#[sim_test]
async fn test_simulate_resolves_omitted_budget_from_gas_allowance() {
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_rpc::proto::sui::rpc::v2::{
        GasPayment, Input, ProgrammableTransaction, SimulateTransactionRequest, Transaction,
        TransactionKind,
    };
    use sui_types::crypto::{AccountKeyPair, get_key_pair};

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let mut env = TestEnvBuilder::new().build().await;
    let (spender, _): (_, AccountKeyPair) = get_key_pair();
    let (allowance_id, _) = issue_allowance(&mut env, spender, 100_000_000).await;

    let mut transaction = Transaction::default();
    transaction.kind = Some(TransactionKind::from({
        let mut pt = ProgrammableTransaction::default();
        let mut allowance = Input::default();
        allowance.object_id = Some(allowance_id.to_canonical_string(true));
        allowance.mutable = Some(true);
        pt.inputs = vec![allowance];
        pt
    }));
    transaction.sender = Some(spender.to_string());
    transaction.gas_payment = Some({
        let mut payment = GasPayment::default();
        payment.owner = Some(allowance_id.to_canonical_string(true));
        payment
    });

    let mut client = TransactionExecutionServiceClient::connect(env.cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let response = client
        .simulate_transaction(
            SimulateTransactionRequest::new(transaction).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    let resolved: TransactionData = response
        .transaction()
        .transaction()
        .bcs()
        .deserialize()
        .unwrap();

    assert_eq!(resolved.gas_data().owner, allowance_id.into());
    assert!(resolved.gas_data().payment.is_empty());
    assert!(resolved.gas_data().budget <= 100_000_000);
    assert!(response.transaction().effects().status().success());
}

// =============================================================================
// Regression: an allowance-paid Coin<SUI> carries its funder's protected storage
// deposit. At low RGP, mock-coin estimation must not omit the replacement deposit
// when that coin is selected to pay ordinary gas.
// =============================================================================

#[sim_test]
async fn test_omitted_budget_accounts_for_protected_selected_gas_coin() {
    use rand::rngs::OsRng;
    use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
    use sui_rpc::proto::sui::rpc::v2::{
        GasPayment, ProgrammableTransaction, SimulateTransactionRequest, Transaction,
        TransactionKind,
    };
    use sui_swarm_config::genesis_config::ValidatorGenesisConfigBuilder;
    use sui_types::{crypto::get_key_pair, gas_coin::GasCoin};

    const LOW_REFERENCE_GAS_PRICE: u64 = 1;
    const GAS_SAFE_OVERHEAD: u64 = 1_000;

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let mut env = TestEnvBuilder::new()
        .with_test_cluster_builder_cb(Box::new(|builder| {
            builder.with_validators(vec![
                ValidatorGenesisConfigBuilder::new()
                    .with_gas_price(LOW_REFERENCE_GAS_PRICE)
                    .build(&mut OsRng),
            ])
        }))
        .build()
        .await;
    assert_eq!(env.rgp, LOW_REFERENCE_GAS_PRICE);

    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let (allowance_id, initial_shared_version) =
        issue_allowance(&mut env, spender, 100_000_000).await;
    let mut client = TransactionExecutionServiceClient::connect(env.cluster.rpc_url().to_owned())
        .await
        .unwrap();

    for explicit_payment in [false, true] {
        let (funder, protected_gas_coin) = create_protected_gas_coin(
            &mut env,
            spender,
            &key,
            (allowance_id, initial_shared_version),
            5_000_000,
            if explicit_payment { 1 } else { 0 },
        )
        .await;
        let protected_coin = env
            .cluster
            .get_object_from_fullnode_store(&protected_gas_coin.0)
            .await
            .unwrap();
        let initial_balance = GasCoin::try_from(&protected_coin).unwrap().value();
        let marker = env
            .cluster
            .get_object_from_fullnode_store(&sui_types::gas_allowance::StorageRebate::child_id(
                protected_gas_coin.0,
            ))
            .await
            .expect("the allowance-paid coin must retain its protected deposit marker");
        let protected_rebate: sui_types::gas_allowance::StorageRebate =
            bcs::from_bytes(marker.data.try_as_move().unwrap().contents()).unwrap();
        assert!(
            protected_rebate.amount > GAS_SAFE_OVERHEAD,
            "the protected replacement deposit must exceed the estimator's old safety buffer"
        );

        let mut transaction = Transaction::default();
        transaction.kind = Some(TransactionKind::from(ProgrammableTransaction::default()));
        transaction.sender = Some(spender.to_string());
        transaction.gas_payment = Some({
            let mut payment = GasPayment::default();
            payment.owner = Some(spender.to_string());
            if explicit_payment {
                payment.objects = vec![object_ref_to_proto(&protected_gas_coin)];
            }
            payment
        });
        let response = client
            .simulate_transaction(
                SimulateTransactionRequest::new(transaction).with_do_gas_selection(true),
            )
            .await
            .unwrap()
            .into_inner();
        let resolved: TransactionData = response
            .transaction()
            .transaction()
            .bcs()
            .deserialize()
            .unwrap();
        assert!(
            response.transaction().effects().status().success(),
            "simulation should complete with the returned budget (explicit_payment={explicit_payment})"
        );
        assert!(
            resolved
                .gas_data()
                .payment
                .iter()
                .any(|payment| payment.0 == protected_gas_coin.0),
            "gas selection must retain the protected payment coin"
        );

        let (execution_digest, effects) = env
            .cluster
            .execute_transaction_directly(&SignedTransaction::from_data_and_signer(
                resolved.clone(),
                vec![&key],
            ))
            .await
            .unwrap();
        env.cluster
            .wait_for_tx_settlement(&[execution_digest])
            .await;
        assert!(effects.status().is_ok(), "{:?}", effects.status());
        let final_balance = GasCoin::try_from(
            &env.cluster
                .get_object_from_fullnode_store(&protected_gas_coin.0)
                .await
                .unwrap(),
        )
        .unwrap()
        .value();
        let actual_payer_charge = initial_balance - final_balance;
        assert!(
            actual_payer_charge > GAS_SAFE_OVERHEAD,
            "the selected protected coin should charge more than the old buffer"
        );
        assert!(
            actual_payer_charge <= resolved.gas_data().budget,
            "the returned budget must cover the payer's actual charge"
        );
        let claim_ref = effects
            .created()
            .into_iter()
            .find(|(_, owner)| *owner == Owner::AddressOwner(funder))
            .expect("the original funder must receive the protected refund claim")
            .0;
        let claim = env
            .cluster
            .get_object_from_fullnode_store(&claim_ref.0)
            .await
            .unwrap();
        assert!(
            claim
                .data
                .try_as_move()
                .unwrap()
                .type_()
                .is(&sui_types::gas_allowance::StorageRebate::type_())
        );
        assert_eq!(
            claim.storage_rebate,
            protected_rebate.amount + marker.storage_rebate,
            "the funder's claim must retain both protected deposits"
        );
    }
}

fn unresolved_split_gas_payment(
    sender: SuiAddress,
    payment: sui_types::base_types::ObjectRef,
    price: u64,
) -> sui_rpc::proto::sui::rpc::v2::Transaction {
    const SPLIT_COUNT: u16 = 100;

    let mut ptb = ProgrammableTransactionBuilder::new();
    let amounts = (0..SPLIT_COUNT).map(|_| ptb.pure(1u64).unwrap()).collect();
    let Argument::Result(split_index) = ptb.command(sui_types::transaction::Command::SplitCoins(
        Argument::GasCoin,
        amounts,
    )) else {
        panic!("SplitCoins should return a result");
    };
    let recipient = ptb.pure(SuiAddress::random_for_testing_only()).unwrap();
    ptb.command(sui_types::transaction::Command::TransferObjects(
        (0..SPLIT_COUNT)
            .map(|result_index| Argument::NestedResult(split_index, result_index))
            .collect(),
        recipient,
    ));

    let mut unresolved = sui_rpc::proto::sui::rpc::v2::Transaction::from(
        TransactionData::new_programmable(sender, vec![payment], ptb.finish(), MIST_PER_SUI, price),
    );
    unresolved.bcs = None;
    unresolved.digest = None;
    unresolved.expiration = None;
    unresolved.gas_payment.as_mut().unwrap().budget = None;
    unresolved
}

fn unresolved_noop_gas_payment(sender: SuiAddress) -> sui_rpc::proto::sui::rpc::v2::Transaction {
    use sui_rpc::proto::sui::rpc::v2::{
        GasPayment, ProgrammableTransaction, Transaction, TransactionKind,
    };

    let mut transaction = Transaction::default();
    transaction.kind = Some(TransactionKind::from(ProgrammableTransaction::default()));
    transaction.sender = Some(sender.to_string());
    transaction.gas_payment = Some({
        let mut payment = GasPayment::default();
        payment.owner = Some(sender.to_string());
        payment
    });
    transaction
}

async fn fund_explicit_gas_payment(
    test_env: &mut TestEnv,
    amount: u64,
) -> (SuiAddress, sui_types::base_types::ObjectRef) {
    let funder = test_env.get_sender(0);
    let sender = test_env.get_sender(1);
    let funding = test_env
        .tx_builder_with_gas(funder, test_env.get_gas_for_sender(funder)[0])
        .transfer_sui(Some(amount), sender)
        .build();
    let (_, effects) = test_env.exec_tx_directly(funding).await.unwrap();
    let payment = effects
        .created()
        .into_iter()
        .find(|(_, owner)| *owner == sui_types::object::Owner::AddressOwner(sender))
        .expect("funding transfer should create the explicit payment coin")
        .0;
    (sender, payment)
}

#[sim_test]
async fn test_omitted_budget_explicit_payment_preserves_gas_coin_spending() {
    use sui_rpc::proto::sui::rpc::v2::{
        SimulateTransactionRequest,
        transaction_execution_service_client::TransactionExecutionServiceClient,
    };

    let mut test_env = TestEnvBuilder::new().build().await;
    let (sender, payment) = fund_explicit_gas_payment(&mut test_env, MIST_PER_SUI).await;
    let request = unresolved_split_gas_payment(sender, payment, test_env.rgp);

    let mut client =
        TransactionExecutionServiceClient::connect(test_env.cluster.rpc_url().to_owned())
            .await
            .unwrap();
    let response = client
        .simulate_transaction(SimulateTransactionRequest::new(request).with_do_gas_selection(true))
        .await
        .expect("estimation must complete the GasCoin split path")
        .into_inner();
    let resolved: TransactionData = response
        .transaction()
        .transaction()
        .bcs()
        .deserialize()
        .unwrap();
    assert!(response.transaction().effects().status().success());
    assert_eq!(
        resolved.gas_data().payment,
        vec![payment],
        "estimation-only funding must not appear in the resolved transaction"
    );
    assert!(
        resolved.gas_data().budget < MIST_PER_SUI,
        "the resolved budget must leave the selected coin spendable"
    );

    let (_, effects) = test_env
        .cluster
        .sign_and_execute_transaction_directly(&resolved)
        .await
        .expect("resolved transaction should execute");
    assert!(
        effects.status().is_ok(),
        "resolved transaction must retain enough GasCoin for its splits: {:?}",
        effects.status()
    );
}

#[sim_test]
async fn test_underfunded_explicit_payment_quotes_complete_execution() {
    use prost::Message;
    use sui_rpc::proto::google::rpc::{ErrorInfo, Status};
    use sui_rpc::proto::sui::rpc::v2::{
        ErrorReason, SimulateTransactionRequest,
        transaction_execution_service_client::TransactionExecutionServiceClient,
    };

    let mut test_env = TestEnvBuilder::new().build().await;
    let (sender, payment) = fund_explicit_gas_payment(&mut test_env, MIST_PER_SUI).await;
    let funded_request = unresolved_split_gas_payment(sender, payment, test_env.rgp);
    let mut client =
        TransactionExecutionServiceClient::connect(test_env.cluster.rpc_url().to_owned())
            .await
            .unwrap();
    let funded = client
        .simulate_transaction(
            SimulateTransactionRequest::new(funded_request).with_do_gas_selection(true),
        )
        .await
        .unwrap()
        .into_inner();
    assert!(funded.transaction().effects().status().success());
    let funded_transaction: TransactionData = funded
        .transaction()
        .transaction()
        .bcs()
        .deserialize()
        .unwrap();

    test_env.update_all_gas().await;
    const AVAILABLE: u64 = 10_000_000;
    let (sender, payment) = fund_explicit_gas_payment(&mut test_env, AVAILABLE).await;
    let request = unresolved_split_gas_payment(sender, payment, test_env.rgp);
    let error = client
        .simulate_transaction(SimulateTransactionRequest::new(request).with_do_gas_selection(true))
        .await
        .expect_err("underfunded payment must quote the complete execution cost");
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    let details = Status::decode(error.details()).unwrap().details;
    let info = details
        .iter()
        .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .unwrap_or_else(|| panic!("expected an insufficient-funding quote: {error}"));
    assert_eq!(
        info.reason,
        ErrorReason::InsufficientGasBalance.as_str_name()
    );
    assert_eq!(info.domain, ErrorReason::DOMAIN);
    assert!(!info.metadata.contains_key("owner"));
    assert!(!info.metadata.contains_key("coinBalance"));
    assert!(!info.metadata.contains_key("addressBalance"));
    let available: u64 = info.metadata["selectedPaymentBalance"].parse().unwrap();
    let required: u64 = info.metadata["requiredBudget"].parse().unwrap();
    assert_eq!(available, AVAILABLE);
    assert!(required > AVAILABLE);
    assert_eq!(
        required,
        funded_transaction.gas_budget(),
        "the quote must include all split outputs, not just work before out-of-gas"
    );
}

#[sim_test]
async fn test_underfunded_protected_selected_payment_has_structured_quote() {
    use prost::Message;
    use sui_rpc::proto::google::rpc::{ErrorInfo, Status};
    use sui_rpc::proto::sui::rpc::v2::{
        ErrorReason, SimulateTransactionRequest,
        transaction_execution_service_client::TransactionExecutionServiceClient,
    };
    use sui_types::crypto::get_key_pair;

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let mut env = TestEnvBuilder::new().build().await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let mut client = TransactionExecutionServiceClient::connect(env.cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mock_quote = client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_noop_gas_payment(spender))
                .with_do_gas_selection(true),
        )
        .await
        .expect_err("an unfunded account should expose the mock-gas budget");
    let mock_details = Status::decode(mock_quote.details()).unwrap().details;
    let mock_info = mock_details
        .iter()
        .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .unwrap_or_else(|| panic!("expected an insufficient-funding quote: {mock_quote}"));
    assert_eq!(
        mock_info.reason,
        ErrorReason::InsufficientGasBalance.as_str_name()
    );
    let mock_budget: u64 = mock_info.metadata["requiredBudget"].parse().unwrap();

    let allowance = issue_allowance(&mut env, spender, 100_000_000).await;
    create_protected_gas_coin(&mut env, spender, &key, allowance, mock_budget, 0).await;

    let error = client
        .simulate_transaction(
            SimulateTransactionRequest::new(unresolved_noop_gas_payment(spender))
                .with_do_gas_selection(true),
        )
        .await
        .expect_err("the protected payment should be short after re-estimation");
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    let details = Status::decode(error.details()).unwrap().details;
    let info = details
        .iter()
        .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .unwrap_or_else(|| panic!("expected an insufficient-funding quote: {error}"));
    assert_eq!(
        info.reason,
        ErrorReason::InsufficientGasBalance.as_str_name()
    );
    assert_eq!(info.domain, ErrorReason::DOMAIN);
    assert!(!info.metadata.contains_key("owner"));
    assert!(!info.metadata.contains_key("coinBalance"));
    assert!(!info.metadata.contains_key("addressBalance"));
    let available: u64 = info.metadata["selectedPaymentBalance"].parse().unwrap();
    let required: u64 = info.metadata["requiredBudget"].parse().unwrap();
    assert_eq!(available, mock_budget);
    assert!(
        required > available,
        "protected payment re-estimation must exceed the mock-gas estimate"
    );
}

#[sim_test]
async fn test_underfunded_explicit_payment_does_not_quote_a_semantic_failure() {
    use prost::Message;
    use sui_rpc::proto::google::rpc::{ErrorInfo, Status};
    use sui_rpc::proto::sui::rpc::v2::{
        ErrorReason, SimulateTransactionRequest,
        execution_error::ExecutionErrorKind as ProtoExecutionErrorKind,
        transaction_execution_service_client::TransactionExecutionServiceClient,
    };

    let mut env = TestEnvBuilder::new().build().await;
    let (sender, payment) = fund_explicit_gas_payment(&mut env, 1).await;
    let request = unresolved_split_gas_payment(sender, payment, env.rgp);
    let mut client = TransactionExecutionServiceClient::connect(env.cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let error = client
        .simulate_transaction(SimulateTransactionRequest::new(request).with_do_gas_selection(true))
        .await
        .expect_err("the selected coin cannot fund the split commands");
    let details = Status::decode(error.details()).unwrap().details;
    let info = details
        .iter()
        .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .unwrap_or_else(|| panic!("expected execution failure, not a funding quote: {error}"));
    assert_eq!(
        info.reason,
        ErrorReason::TransactionExecutionFailed.as_str_name()
    );
    assert_eq!(
        info.metadata["kind"],
        ProtoExecutionErrorKind::InsufficientCoinBalance.as_str_name()
    );
}

#[sim_test]
async fn test_storage_rebate_markers_are_hidden_from_dynamic_field_listing() {
    use move_core_types::identifier::Identifier;
    use std::path::PathBuf;
    use sui_rpc::proto::sui::rpc::v2::ListDynamicFieldsRequest;
    use sui_rpc::proto::sui::rpc::v2::state_service_client::StateServiceClient;
    use sui_types::{
        crypto::{AccountKeyPair, get_key_pair},
        object::Owner,
        transaction::{
            CallArg, GasData, ObjectArg, ProgrammableTransaction, SharedObjectMutability,
            TransactionDataV1, TransactionExpiration, TransactionKind,
        },
    };

    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let mut env = TestEnvBuilder::new().build().await;
    let (spender, key): (_, AccountKeyPair) = get_key_pair();
    let (allowance_id, initial_shared_version) =
        issue_allowance(&mut env, spender, 500_000_000).await;
    let package = env
        .setup_test_package(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/move_test_code"))
        .await;

    let mut pt = ProgrammableTransactionBuilder::new();
    let value = pt.pure(1u64).unwrap();
    let data = pt.pure(vec![7u8; 16]).unwrap();
    pt.programmable_move_call(
        package,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_large_storage").unwrap(),
        vec![],
        vec![value, data],
    );
    let mut pt: ProgrammableTransaction = pt.finish();
    pt.inputs.push(CallArg::Object(ObjectArg::SharedObject {
        id: allowance_id,
        initial_shared_version,
        mutability: SharedObjectMutability::Mutable,
    }));
    let transaction = TransactionData::V1(TransactionDataV1 {
        kind: TransactionKind::ProgrammableTransaction(pt),
        sender: spender,
        gas_data: GasData {
            payment: vec![],
            owner: allowance_id.into(),
            price: env.rgp,
            budget: 20_000_000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: env.chain_id,
            nonce: 0,
        },
    });
    let signed = sui_types::transaction::Transaction::from_data_and_signer(transaction, vec![&key]);
    let (digest, effects) = env
        .cluster
        .execute_transaction_directly(&signed)
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    env.cluster.wait_for_tx_settlement(&[digest]).await;
    let parent = effects
        .created()
        .into_iter()
        .find_map(|(object, owner)| match owner {
            Owner::AddressOwner(owner) if owner == spender => Some(object.0),
            _ => None,
        })
        .unwrap();
    let marker = sui_types::gas_allowance::StorageRebate::child_id(parent);
    assert!(
        env.cluster
            .get_object_from_fullnode_store(&marker)
            .await
            .is_some()
    );

    let mut client = StateServiceClient::connect(env.cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let full = client
        .list_dynamic_fields(
            ListDynamicFieldsRequest::default()
                .with_parent(parent.to_canonical_string(true))
                .with_page_size(1),
        )
        .await
        .unwrap()
        .into_inner();
    assert!(full.dynamic_fields.is_empty());
    assert!(full.next_page_token.is_none());

    let id_only = client
        .list_dynamic_fields(
            ListDynamicFieldsRequest::default()
                .with_parent(parent.to_canonical_string(true))
                .with_read_mask(prost_types::FieldMask {
                    paths: vec!["field_id".to_owned()],
                }),
        )
        .await
        .unwrap()
        .into_inner();
    assert!(id_only.dynamic_fields.is_empty());
    assert!(id_only.next_page_token.is_none());
}
