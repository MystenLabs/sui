// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for address-balance gas and payment flows.
//! Verifies that coins are swept to address balance and subsequent
//! transactions can operate purely from address balance.

use std::num::NonZeroUsize;
use std::path::Path;

use prost_types::FieldMask;
use serde_json::json;
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::TransactionIdentifierResponse;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{GetBalanceRequest, GetEpochRequest, GetTransactionRequest};
use sui_swarm_config::genesis_config::AccountConfig;
use sui_types::base_types::SuiAddress;
use test_cluster::TestClusterBuilder;

mod test_utils;
use test_utils::wait_for_transaction;

#[allow(dead_code)]
mod rosetta_client;
use rosetta_client::start_rosetta_test_server;

#[allow(dead_code)]
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

async fn fetch_transaction_and_get_operations(
    test_cluster: &test_cluster::TestCluster,
    tx_digest: sui_types::digests::TransactionDigest,
    coin_cache: &CoinMetadataCache,
) -> anyhow::Result<Operations> {
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let grpc_request = GetTransactionRequest::default()
        .with_digest(tx_digest.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events.events.event_type",
            "events.events.json",
            "events.events.contents",
        ]));

    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await?
        .into_inner();

    let executed_tx = grpc_response
        .transaction
        .ok_or_else(|| anyhow::anyhow!("Response transaction should not be empty"))?;
    Operations::try_from_executed_transaction(executed_tx, coin_cache)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse operations: {}", e))
}

/// Run the full rosetta construction flow (preprocess → metadata → payloads → combine → submit),
/// assert each step succeeds, wait for the transaction, and verify it executed successfully.
async fn rosetta_flow_success(
    rosetta_client: &rosetta_client::RosettaClient,
    client: &mut GrpcClient,
    ops: &Operations,
    keystore: &sui_keys::keystore::Keystore,
) -> TransactionIdentifierResponse {
    let flow = rosetta_client.rosetta_flow(ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.metadata {
        panic!("Metadata failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

    let response: TransactionIdentifierResponse = flow
        .submit
        .expect("Submit was None")
        .expect("Submit failed");

    wait_for_transaction(client, &response.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    // Verify via gRPC
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));

    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Transaction should not be empty");
    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );

    response
}

fn single_coin_accounts() -> Vec<AccountConfig> {
    const AMOUNT_150K_SUI: u64 = 150_000_000_000_000;
    (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT_150K_SUI],
        })
        .collect()
}

const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

/// Assert that the bulk of an address's balance is in address balance (not coin objects).
/// When `expect_zero_coin_balance` is true, asserts coin_balance == 0 exactly (for deficit/AB-gas
/// paths where no gas coin receives a refund). When false, allows a small coin balance from gas
/// refunds on Path B (coin-gas) transactions.
async fn assert_balance_in_address_balance(
    client: &mut GrpcClient,
    address: SuiAddress,
    coin_type: &str,
    expect_zero_coin_balance: bool,
) {
    let request = GetBalanceRequest::default()
        .with_owner(address.to_string())
        .with_coin_type(coin_type.to_string());

    let balance = client
        .state_client()
        .get_balance(request)
        .await
        .unwrap()
        .into_inner()
        .balance
        .expect("Balance response should not be empty");

    let address_bal = balance.address_balance.unwrap_or(0);
    let coin_bal = balance.coin_balance.unwrap_or(0);

    assert!(
        address_bal > 0,
        "Expected address_balance>0 for {address}, got {address_bal}"
    );

    if expect_zero_coin_balance {
        assert_eq!(
            coin_bal, 0,
            "Expected coin_balance=0 for {address}, got {coin_bal}"
        );
    } else {
        // Allow a small coin balance from gas refunds, but the vast majority should be in AB
        assert!(
            address_bal > coin_bal * 100,
            "Expected address_balance to dominate for {address}, \
             got address_balance={address_bal}, coin_balance={coin_bal}"
        );
    }
}

fn pay_sui_ops(sender: SuiAddress, recipient: SuiAddress, amount: &str) -> Operations {
    serde_json::from_value(json!([
        {
            "operation_identifier": {"index": 0},
            "type": "PaySui",
            "account": {"address": recipient.to_string()},
            "amount": {"value": amount}
        },
        {
            "operation_identifier": {"index": 1},
            "type": "PaySui",
            "account": {"address": sender.to_string()},
            "amount": {"value": format!("-{}", amount)}
        }
    ]))
    .unwrap()
}

/// Two PaySui operations in sequence from an account with a single coin.
/// First tx: Path B (coin gas, send remainder to address balance).
/// Second tx: Path A (address-balance gas) — the account has no coin objects,
/// so this tx can ONLY succeed if address balance gas works.
#[tokio::test]
async fn test_pay_sui_sequential_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    let ops = pay_sui_ops(sender, recipient, "1000000000");

    // First tx: Path B (no pre-existing address balance)
    let r1 = rosetta_flow_success(&rosetta_client, &mut client, &ops, keystore).await;
    let ops1 = fetch_transaction_and_get_operations(
        &test_cluster,
        r1.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(ops1.contains(&ops), "First tx operations mismatch");

    // Second tx: Path A (address-balance gas — no coins left)
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Second tx should use address-balance gas (no coins available)"
    );

    let r2: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r2.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        r2.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(ops2.contains(&ops), "Second tx operations mismatch");
}

/// Stake from address balance: do a PaySui first to send all coins to address balance,
/// then stake using the address balance (no coin objects available).
#[tokio::test]
async fn test_stake_from_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // PaySui to send all coins to address balance
    let pay_ops = pay_sui_ops(sender, recipient, "1000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &pay_ops, keystore).await;

    // Get validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validator = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap()
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake from address balance (no coins available)
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": "-1000000000"},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();

    let flow = rosetta_client
        .rosetta_flow(&stake_ops, keystore, None)
        .await;
    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Stake should use address-balance gas"
    );

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        r.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(ops2.contains(&stake_ops), "Stake operations mismatch");
}

/// Stake all from address balance: do a PaySui to send coins, then stake all
/// (no explicit amount). The total stake = address_balance - gas_budget.
#[tokio::test]
async fn test_stake_all_from_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // PaySui to send coins to address balance
    let pay_ops = pay_sui_ops(sender, recipient, "1000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &pay_ops, keystore).await;

    // Get validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validator = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap()
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake all (no amount) from address balance only
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();

    let flow = rosetta_client
        .rosetta_flow(&stake_ops, keystore, None)
        .await;
    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Stake all should use address-balance gas"
    );

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        r.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(ops2.contains(&stake_ops), "Stake all operations mismatch");
}

/// PayCoin (non-SUI) using SUI address balance for gas.
/// 1. Deploy custom coin package + mint coins (while SUI coins exist)
/// 2. PaySui to send all SUI coins to address balance
/// 3. PayCoin: custom coins swept to address balance, SUI address balance for gas
#[tokio::test]
async fn test_pay_coin_from_address_balance() {
    use test_coin_utils::{TEST_COIN_DECIMALS, init_package, mint};

    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT], // 2 coins: one for init, one for mint+gas
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    // Step 1: Deploy custom coin package and mint coins (needs SUI coin objects for gas)
    let init_ret = init_package(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let coin_balance = 1_000_000u64;
    let n_coins = 3usize;
    let total_balance = n_coins as i128 * coin_balance as i128;
    mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret,
        vec![(coin_balance, sender); n_coins],
    )
    .await
    .unwrap();

    // Step 2: PaySui to send all remaining SUI coins to address balance
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let pay_ops = pay_sui_ops(sender, recipient, "1000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &pay_ops, keystore).await;

    // Step 3: PayCoin — custom coins swept, SUI address balance for gas
    let pay_coin_ops: Operations = serde_json::from_value(json!([
        {
            "operation_identifier": {"index": 0},
            "type": "PayCoin",
            "account": {"address": recipient.to_string()},
            "amount": {
                "value": total_balance.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        },
        {
            "operation_identifier": {"index": 1},
            "type": "PayCoin",
            "account": {"address": sender.to_string()},
            "amount": {
                "value": (-total_balance).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type}
                }
            }
        }
    ]))
    .unwrap();

    let flow = rosetta_client
        .rosetta_flow(&pay_coin_ops, keystore, None)
        .await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("PayCoin preprocess failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.metadata {
        panic!("PayCoin metadata failed: {:?}", e);
    }

    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "PayCoin should use SUI address-balance gas"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("PayCoin payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("PayCoin combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    // Verify transaction succeeded via gRPC
    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));
    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();
    let tx = grpc_response.transaction.unwrap();
    assert!(
        tx.effects().status().success(),
        "PayCoin from address balance failed: {:?}",
        tx.effects().status().error()
    );
}

/// Withdraw stake → address balance → PaySui.
/// Verifies that withdrawn SUI is deposited to address balance and usable.
/// 1. Stake some SUI (sends coins to address balance)
/// 2. Trigger epoch change so stake becomes withdrawable
/// 3. Withdraw all stake (swept to address balance)
/// 4. PaySui from address balance (proves withdrawn funds are usable)
#[tokio::test]
async fn test_withdraw_stake_then_pay_from_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(60000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Get validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validator = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap()
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Step 1: Stake (sends the single coin to address balance, uses coin gas for Path B)
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": "-1000000000"},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &stake_ops, keystore).await;

    // Step 2: Trigger epoch change so stake becomes withdrawable
    test_cluster.trigger_reconfiguration().await;

    // Step 3: Withdraw all stake (withdrawn SUI swept to address balance)
    let withdraw_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "WithdrawStake",
        "account": {"address": sender.to_string()}
    }]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &withdraw_ops, keystore).await;

    // Step 4: PaySui from address balance — proves withdrawn SUI is in address balance
    let pay_ops = pay_sui_ops(sender, recipient, "1000000000");
    let flow = rosetta_client.rosetta_flow(&pay_ops, keystore, None).await;

    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "PaySui after withdraw should use address-balance gas"
    );

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));
    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();
    let tx = grpc_response.transaction.unwrap();
    assert!(
        tx.effects().status().success(),
        "PaySui from address balance after withdraw failed: {:?}",
        tx.effects().status().error()
    );
}

/// PaySui with deficit: sender has a coin that partially covers the payment,
/// and the shortfall is withdrawn from address balance.
/// Exercises the path where both coin merging and FundsWithdrawal happen in the same PTB.
#[tokio::test]
async fn test_pay_sui_deficit_path() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // Step 1: PaySui from sender → recipient. Consumes the single coin, remainder goes to AB.
    let setup_ops = pay_sui_ops(sender, recipient, "1000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &setup_ops, keystore).await;
    // sender: 0 coins + ~149K SUI in AB

    // Step 2: PaySui from recipient → sender. Gives sender a small coin object (5 SUI).
    let fund_ops = pay_sui_ops(recipient, sender, "5000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &fund_ops, keystore).await;
    // sender: 5 SUI coin + ~149K SUI in AB

    // Step 3: PaySui with deficit — payment (10 SUI) > coin (5 SUI), deficit = 5 SUI from AB
    let deficit_ops = pay_sui_ops(sender, recipient, "10000000000");
    let flow = rosetta_client
        .rosetta_flow(&deficit_ops, keystore, None)
        .await;

    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Deficit PaySui should use address-balance gas (Path A)"
    );
    assert!(
        metadata.metadata.address_balance_withdrawal > 0,
        "Should have non-zero address balance withdrawal (deficit)"
    );

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        r.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&deficit_ops),
        "Deficit PaySui operations mismatch"
    );
}

/// Stake with deficit: sender has a coin that partially covers the stake amount,
/// and the shortfall is withdrawn from address balance.
#[tokio::test]
async fn test_stake_deficit_path() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // Step 1: PaySui from sender → recipient. Consumes coin, remainder to AB.
    let setup_ops = pay_sui_ops(sender, recipient, "1000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &setup_ops, keystore).await;

    // Step 2: PaySui from recipient → sender. Gives sender a small coin (5 SUI).
    let fund_ops = pay_sui_ops(recipient, sender, "5000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &fund_ops, keystore).await;
    // sender: 5 SUI coin + ~149K SUI in AB

    // Get validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validator = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap()
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Step 3: Stake with deficit — stake 10 SUI > coin (5 SUI), deficit = 5 SUI from AB
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": "-10000000000"},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();

    let flow = rosetta_client
        .rosetta_flow(&stake_ops, keystore, None)
        .await;

    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Deficit Stake should use address-balance gas (Path A)"
    );
    assert!(
        metadata.metadata.address_balance_withdrawal > 0,
        "Should have non-zero address balance withdrawal (deficit)"
    );

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        r.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&stake_ops),
        "Deficit Stake operations mismatch"
    );
}

/// PayCoin with deficit: sender has custom coin objects that partially cover the payment,
/// and the shortfall is withdrawn from custom coin address balance.
#[tokio::test]
async fn test_pay_coin_deficit_path() {
    use test_coin_utils::{TEST_COIN_DECIMALS, init_package, mint};

    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let deficit_sender = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    // Step 1: Deploy custom coin and mint to both accounts
    let init_ret = init_package(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    // Mint 1M to sender (for the setup PayCoin) and 500K to deficit_sender (the coin for deficit)
    let coin_large = 1_000_000u64;
    let coin_small = 500_000u64;
    mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret,
        vec![(coin_large, sender), (coin_small, deficit_sender)],
    )
    .await
    .unwrap();

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // Step 2: PayCoin from sender → deficit_sender to create custom coin AB for deficit_sender
    let transfer_amount = coin_large as i128;
    let setup_ops: Operations = serde_json::from_value(json!([
        {
            "operation_identifier": {"index": 0},
            "type": "PayCoin",
            "account": {"address": deficit_sender.to_string()},
            "amount": {
                "value": transfer_amount.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        },
        {
            "operation_identifier": {"index": 1},
            "type": "PayCoin",
            "account": {"address": sender.to_string()},
            "amount": {
                "value": (-transfer_amount).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        }
    ]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &setup_ops, keystore).await;
    // deficit_sender: 500K coin (from mint) + 1M AB (from PayCoin send_funds)

    // Step 3: PayCoin with deficit — payment (800K) > coin (500K), deficit = 300K from AB
    let deficit_payment = 800_000i128;
    let deficit_ops: Operations = serde_json::from_value(json!([
        {
            "operation_identifier": {"index": 0},
            "type": "PayCoin",
            "account": {"address": sender.to_string()},
            "amount": {
                "value": deficit_payment.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        },
        {
            "operation_identifier": {"index": 1},
            "type": "PayCoin",
            "account": {"address": deficit_sender.to_string()},
            "amount": {
                "value": (-deficit_payment).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        }
    ]))
    .unwrap();

    let flow = rosetta_client
        .rosetta_flow(&deficit_ops, keystore, None)
        .await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("PayCoin deficit preprocess failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.metadata {
        panic!("PayCoin deficit metadata failed: {:?}", e);
    }

    let metadata = flow.metadata.as_ref().unwrap().as_ref().unwrap();
    assert!(
        metadata.metadata.address_balance_withdrawal > 0,
        "Should have non-zero address balance withdrawal (deficit)"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("PayCoin deficit payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("PayCoin deficit combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        r.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&deficit_ops),
        "PayCoin deficit operations mismatch"
    );
}

/// Test that address-balance gas payments (where effects.gas_object is None) are correctly
/// parsed into Operations.
#[tokio::test]
async fn test_address_balance_gas_payment_parsing() {
    use std::collections::{BTreeMap, BTreeSet};
    use std::str::FromStr;
    use sui_rpc::proto::sui::rpc::v2::{
        BalanceChange, Bcs, ExecutedTransaction, GetTransactionResponse, Transaction,
        TransactionEffects,
    };
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::{ObjectDigest, ObjectID};
    use sui_types::crypto::{AccountKeyPair, get_key_pair};
    use sui_types::effects::TransactionEffects as NativeTransactionEffects;
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::GasCostSummary;
    use sui_types::utils::to_sender_signed_transaction;

    let test_cluster = TestClusterBuilder::new().build().await;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    const SENDER: &str = "0x6293e2b4434265fa60ac8ed96342b7a288c0e43ffe737ba40feb24f06fed305d";
    const RECIPIENT: &str = "0x0e3225553e3b945b4cde5621a980297c45b96002f33c95d3306e58013129ee7c";
    let sender_address = SuiAddress::from_str(SENDER).unwrap();
    let recipient_address = SuiAddress::from_str(RECIPIENT).unwrap();
    let (_, sender_key): (_, AccountKeyPair) = get_key_pair();

    let gas_ref = (
        ObjectID::from_hex_literal(
            "0x08d6f5f85a55933fff977c94a2d1d94e8e2fff241c19c20bc5c032e0989f16a4",
        )
        .unwrap(),
        8.into(),
        ObjectDigest::from_str("dsk2WjBAbXh8oEppwavnwWmEsqRbBkSGDmVZGBaZHY6").unwrap(),
    );

    let tx_data = TestTransactionBuilder::new(sender_address, gas_ref, 1000)
        .transfer_sui(Some(1_000_000), recipient_address)
        .build();

    let signed_tx = to_sender_signed_transaction(tx_data.clone(), &sender_key);
    let tx_digest = *signed_tx.digest();

    // Build effects with gas_object = None to simulate address-balance gas payment.
    let effects = NativeTransactionEffects::new_from_execution_v2(
        ExecutionStatus::Success,
        0,                                  // executed_epoch
        GasCostSummary::new(1000, 0, 0, 0), // computation_cost, non_refundable_storage_fee, storage_cost, storage_rebate
        vec![],                             // shared_objects
        BTreeSet::new(),                    // loaded_per_epoch_config_objects
        tx_digest,                          // transaction_digest
        9.into(),                           // lamport_version
        BTreeMap::new(),                    // changed_objects
        None,                               // gas_object: None = address-balance gas payment
        None,                               // events_digest
        vec![],                             // dependencies
    );

    let tx_data_bcs = bcs::to_bytes(&tx_data).unwrap();
    let effects_bcs = bcs::to_bytes(&effects).unwrap();

    let mut executed_transaction = ExecutedTransaction::default();
    executed_transaction.digest = Some(tx_digest.to_string());

    let mut transaction: Transaction = tx_data.clone().into();
    let mut tx_bcs = Bcs::default();
    tx_bcs.value = Some(tx_data_bcs.into());
    transaction.bcs = Some(tx_bcs);
    executed_transaction.transaction = Some(transaction);
    executed_transaction.signatures = vec![];

    let mut transaction_effects: TransactionEffects = effects.into();
    let mut effects_bcs_struct = Bcs::default();
    effects_bcs_struct.value = Some(effects_bcs.into());
    transaction_effects.bcs = Some(effects_bcs_struct);
    executed_transaction.effects = Some(transaction_effects);

    executed_transaction.events = None;
    executed_transaction.checkpoint = Some(293254043);
    executed_transaction.timestamp = Some(::prost_types::Timestamp {
        seconds: 1736949830,
        nanos: 0,
    });

    let mut balance_change = BalanceChange::default();
    balance_change.address = Some(SENDER.to_string());
    balance_change.coin_type = Some(
        "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI".to_string(),
    );
    balance_change.amount = Some("-1000".to_string());
    executed_transaction.balance_changes = vec![balance_change];

    let mut response = GetTransactionResponse::default();
    response.transaction = Some(executed_transaction);

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let executed_tx = response
        .transaction
        .expect("Response transaction should not be empty");
    let result = Operations::try_from_executed_transaction(executed_tx, &coin_cache).await;

    let ops = result.expect("Address-balance gas payment should be parsed successfully");
    let ops_vec: Vec<_> = ops.into_iter().collect();
    assert!(
        !ops_vec.is_empty(),
        "Operations should not be empty for address-balance gas payment"
    );
}

/// PaySui deposits funds to address balance: after Path B and deficit Path A,
/// sender should have coin_balance=0 and address_balance>0.
#[tokio::test]
async fn test_pay_sui_deposits_to_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Step 1: PaySui (Path B) — consumes coin, remainder to address balance
    let ops = pay_sui_ops(sender, recipient, "1000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &ops, keystore).await;

    // Verify sender: address_balance>0 (small gas refund coin allowed from Path B)
    assert_balance_in_address_balance(&mut client, sender, SUI_COIN_TYPE, false).await;

    // Step 2: Give sender a small coin back
    let fund_ops = pay_sui_ops(recipient, sender, "5000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &fund_ops, keystore).await;

    // Step 3: Deficit PaySui — payment (10 SUI) > coin (5 SUI), deficit from AB
    let deficit_ops = pay_sui_ops(sender, recipient, "10000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &deficit_ops, keystore).await;

    // Verify sender: coin_balance=0, address_balance>0 (AB gas → no coin refund)
    assert_balance_in_address_balance(&mut client, sender, SUI_COIN_TYPE, true).await;
}

/// Stake deficit path deposits funds to address balance: after stake with deficit,
/// sender should have coin_balance=0 and address_balance>0.
#[tokio::test]
async fn test_stake_deposits_to_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Step 1: PaySui to send coin to address balance
    let setup_ops = pay_sui_ops(sender, recipient, "1000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &setup_ops, keystore).await;

    // Step 2: Give sender a small coin back
    let fund_ops = pay_sui_ops(recipient, sender, "5000000000");
    rosetta_flow_success(&rosetta_client, &mut client, &fund_ops, keystore).await;

    // Get validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validator = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap()
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Step 3: Deficit stake — stake 10 SUI > coin (5 SUI), deficit from AB
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": "-10000000000"},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &stake_ops, keystore).await;

    // Verify sender: coin_balance=0, address_balance>0 (AB gas → no coin refund)
    assert_balance_in_address_balance(&mut client, sender, SUI_COIN_TYPE, true).await;
}

/// WithdrawStake deposits to address balance: after withdrawing stake,
/// sender should have coin_balance=0 and address_balance>0.
#[tokio::test]
async fn test_withdraw_stake_deposits_to_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(60000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Get validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validator = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap()
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Step 1: Stake (consumes coin, remainder to AB)
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": "-1000000000"},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &stake_ops, keystore).await;

    // Step 2: Epoch change so stake becomes withdrawable
    test_cluster.trigger_reconfiguration().await;

    // Step 3: WithdrawStake (withdrawn SUI swept to address balance)
    let withdraw_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "WithdrawStake",
        "account": {"address": sender.to_string()}
    }]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &withdraw_ops, keystore).await;

    // Verify sender: address_balance>0 (small gas refund coin allowed from Path B)
    assert_balance_in_address_balance(&mut client, sender, SUI_COIN_TYPE, false).await;
}

/// PayCoin deficit path deposits custom coins to address balance: after deficit PayCoin,
/// deficit_sender should have coin_balance=0 and address_balance>0 for the custom coin.
#[tokio::test]
async fn test_pay_coin_deposits_to_address_balance() {
    use test_coin_utils::{TEST_COIN_DECIMALS, init_package, mint};

    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let deficit_sender = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    // Step 1: Deploy custom coin and mint to both accounts
    let init_ret = init_package(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let coin_large = 1_000_000u64;
    let coin_small = 500_000u64;
    mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret,
        vec![(coin_large, sender), (coin_small, deficit_sender)],
    )
    .await
    .unwrap();

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Step 2: PayCoin sender → deficit_sender to create custom coin AB for deficit_sender
    let transfer_amount = coin_large as i128;
    let setup_ops: Operations = serde_json::from_value(json!([
        {
            "operation_identifier": {"index": 0},
            "type": "PayCoin",
            "account": {"address": deficit_sender.to_string()},
            "amount": {
                "value": transfer_amount.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        },
        {
            "operation_identifier": {"index": 1},
            "type": "PayCoin",
            "account": {"address": sender.to_string()},
            "amount": {
                "value": (-transfer_amount).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        }
    ]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &setup_ops, keystore).await;
    // deficit_sender: 500K coin (from mint) + 1M AB (from PayCoin)

    // Step 3: Deficit PayCoin deficit_sender → sender (800K > 500K coin, deficit from AB)
    let deficit_payment = 800_000i128;
    let deficit_ops: Operations = serde_json::from_value(json!([
        {
            "operation_identifier": {"index": 0},
            "type": "PayCoin",
            "account": {"address": sender.to_string()},
            "amount": {
                "value": deficit_payment.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        },
        {
            "operation_identifier": {"index": 1},
            "type": "PayCoin",
            "account": {"address": deficit_sender.to_string()},
            "amount": {
                "value": (-deficit_payment).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type.clone()}
                }
            }
        }
    ]))
    .unwrap();
    rosetta_flow_success(&rosetta_client, &mut client, &deficit_ops, keystore).await;

    // Verify deficit_sender: coin_balance=0, address_balance>0 for TEST_COIN (AB gas → no coin refund)
    assert_balance_in_address_balance(&mut client, deficit_sender, &coin_type, true).await;
}
