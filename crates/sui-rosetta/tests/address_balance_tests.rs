// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for address-balance gas and payment flows.
//! Verifies that coins are swept to address balance and subsequent
//! transactions can operate purely from address balance.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::path::Path;
use std::str::FromStr;

use prost_types::FieldMask;
use serde_json::json;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{PreprocessMetadata, TransactionIdentifierResponse};
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{GetBalanceRequest, GetEpochRequest, GetTransactionRequest};
use sui_sdk_types::{Address, TypeTag as SdkTypeTag};
use sui_swarm_config::genesis_config::AccountConfig;
use sui_types::base_types::SuiAddress;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_sdk_types_conversions::type_tag_sdk_to_core;
use sui_types::transaction::{Command, TransactionData, TransactionDataAPI};
use sui_types::{Identifier, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;

mod test_utils;
use test_coin_utils::{TEST_COIN_DECIMALS, init_package, mint};
use test_utils::wait_for_transaction;

mod rosetta_client;
use rosetta_client::start_rosetta_test_server;

#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

const SUI_COIN_TYPE: &str =
    "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI";

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

async fn get_total_balance(client: &mut GrpcClient, address: SuiAddress, coin_type: &str) -> u64 {
    let request = GetBalanceRequest::default()
        .with_owner(address.to_string())
        .with_coin_type(coin_type.to_string());

    client
        .state_client()
        .get_balance(request)
        .await
        .unwrap()
        .into_inner()
        .balance()
        .balance()
}

/// Return only the address-balance portion of an account's holdings (excludes coin objects),
/// so a test can assert a recipient was credited to address balance rather than a coin object.
async fn get_address_balance(client: &mut GrpcClient, address: SuiAddress, coin_type: &str) -> u64 {
    let request = GetBalanceRequest::default()
        .with_owner(address.to_string())
        .with_coin_type(coin_type.to_string());

    client
        .state_client()
        .get_balance(request)
        .await
        .unwrap()
        .into_inner()
        .balance()
        .address_balance()
}

/// Assert an executed transaction ran on the free tier: no gas payment objects, price 0, budget 0.
/// This is the protocol's definition of gasless. We assert the `gas_payment` rather than the cost
/// summary because, for gasless txs, the summary's `computation_cost`/`storage_rebate` carry the
/// storage rebate absorbed from destroyed input coins (see `gas_charger.rs`) — a subsidized amount,
/// not a charge — so they can be non-zero even though the sender pays nothing.
fn assert_onchain_gasless(tx: &sui_rpc::proto::sui::rpc::v2::ExecutedTransaction) {
    let gas_payment = tx.transaction().gas_payment();
    assert_eq!(
        gas_payment.price,
        Some(0),
        "gasless: on-chain gas price must be 0"
    );
    assert_eq!(
        gas_payment.budget,
        Some(0),
        "gasless: on-chain gas budget must be 0"
    );
    assert!(
        gas_payment.objects.is_empty(),
        "gasless: there must be no gas payment objects"
    );
}

/// Deposit coins to address balance via `coin::send_funds<T>`.
/// When `coin_type` is `None`, deposits SUI (split from GasCoin).
/// When `coin_type` is `Some(type_str)`, looks up a custom coin object to split from.
async fn deposit_to_address_balance(
    test_cluster: &test_cluster::TestCluster,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    amount: u64,
    coin_type: Option<&str>,
) {
    let mut ptb = ProgrammableTransactionBuilder::new();
    let mut forbidden = BTreeSet::new();

    let (split_source, type_tag) = match coin_type {
        None => (
            sui_types::transaction::Argument::GasCoin,
            sui_types::gas_coin::GAS::type_tag(),
        ),
        Some(ct) => {
            let sdk_type = SdkTypeTag::from_str(ct).unwrap();
            let all_coins = client
                .select_up_to_n_largest_coins(&Address::from(sender), &sdk_type, 1500, &[])
                .await
                .unwrap();
            assert!(!all_coins.is_empty(), "No coins found for type {ct}");

            let coin_ref = all_coins[0].object_reference().try_to_object_ref().unwrap();
            forbidden.insert(coin_ref.0);
            let arg = ptb
                .obj(sui_types::transaction::ObjectArg::ImmOrOwnedObject(
                    coin_ref,
                ))
                .unwrap();
            (arg, type_tag_sdk_to_core(sdk_type).unwrap())
        }
    };

    let amount_arg = ptb.pure(amount).unwrap();
    let split_coin = ptb.command(Command::SplitCoins(split_source, vec![amount_arg]));
    let recipient_arg = ptb.pure(sender).unwrap();
    ptb.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::from(sui_types::coin::COIN_MODULE_NAME),
        Identifier::new("send_funds").unwrap(),
        vec![type_tag],
        vec![split_coin, recipient_arg],
    ));
    let pt = ptb.finish();

    let price = client.get_reference_gas_price().await.unwrap();
    let budget = 500_000_000u64;
    let (_, gas_object_data) = test_cluster
        .wallet
        .gas_for_owner_budget(sender, budget, forbidden)
        .await
        .unwrap();
    let gas_object = gas_object_data.compute_object_reference();

    let tx_data = TransactionData::new_programmable(sender, vec![gas_object], pt, budget, price);
    let sig = keystore
        .sign_secure(
            &tx_data.sender(),
            &tx_data,
            shared_crypto::intent::Intent::sui_transaction(),
        )
        .await
        .unwrap();
    let signed_tx = sui_types::transaction::Transaction::from_data(tx_data, vec![sig]);
    let response = test_utils::execute_transaction(client, &signed_tx)
        .await
        .unwrap();
    assert!(
        response.effects().status().success(),
        "deposit_to_address_balance failed: {:?}",
        response.effects().status().error()
    );
}

/// Sweep all SUI coin objects to address balance.
/// First deposits most of the balance (building up AB), then sends remaining coins
/// directly to AB using other coins for gas, until only one coin remains.
/// The last coin is sent directly to AB via `coin::send_funds` with AB providing gas
/// (empty gas vector in TransactionData).
async fn sweep_all_coins_to_ab(
    test_cluster: &test_cluster::TestCluster,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
) {
    // Phase 1: deposit most of balance to establish AB (needs coin gas).
    deposit_to_address_balance(
        test_cluster,
        client,
        keystore,
        sender,
        100_000_000_000,
        None,
    )
    .await;

    // Phase 2: send remaining coins directly to AB using AB gas (empty gas vector).
    loop {
        let coins = client
            .select_up_to_n_largest_coins(
                &Address::from(sender),
                &sui_sdk_types::StructTag::sui().into(),
                100,
                &[],
            )
            .await
            .unwrap();
        if coins.is_empty() {
            break;
        }

        let mut ptb = ProgrammableTransactionBuilder::new();
        let recipient_arg = ptb.pure(sender).unwrap();
        for coin in &coins {
            let coin_ref = coin.object_reference().try_to_object_ref().unwrap();
            let coin_arg = ptb
                .obj(sui_types::transaction::ObjectArg::ImmOrOwnedObject(
                    coin_ref,
                ))
                .unwrap();
            ptb.command(Command::move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                Identifier::from(sui_types::coin::COIN_MODULE_NAME),
                Identifier::new("send_funds").unwrap(),
                vec![sui_types::gas_coin::GAS::type_tag()],
                vec![coin_arg, recipient_arg],
            ));
        }
        let pt = ptb.finish();

        let price = client.get_reference_gas_price().await.unwrap();
        let budget = 500_000_000u64;
        // Empty gas vector → AB provides gas
        let tx_data = TransactionData::new_programmable(sender, vec![], pt, budget, price);
        let sig = keystore
            .sign_secure(
                &tx_data.sender(),
                &tx_data,
                shared_crypto::intent::Intent::sui_transaction(),
            )
            .await
            .unwrap();
        let signed_tx = sui_types::transaction::Transaction::from_data(tx_data, vec![sig]);
        let response = test_utils::execute_transaction(client, &signed_tx)
            .await
            .unwrap();
        assert!(
            response.effects().status().success(),
            "sweep_all_coins_to_ab failed: {:?}",
            response.effects().status().error()
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

fn pay_coin_ops(
    sender: SuiAddress,
    recipient: SuiAddress,
    amount: u64,
    coin_type: &str,
) -> Operations {
    let amount_i128 = amount as i128;
    serde_json::from_value(json!([
        {
            "operation_identifier": {"index": 0},
            "type": "PayCoin",
            "account": {"address": recipient.to_string()},
            "amount": {
                "value": amount_i128.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type}
                }
            }
        },
        {
            "operation_identifier": {"index": 1},
            "type": "PayCoin",
            "account": {"address": sender.to_string()},
            "amount": {
                "value": (-amount_i128).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type}
                }
            }
        }
    ]))
    .unwrap()
}

/// Multi-recipient PayCoin operations: one positive op per recipient and a single negative op on
/// the sender for the total.
fn pay_coin_ops_multi(
    sender: SuiAddress,
    recipients: &[(SuiAddress, u64)],
    coin_type: &str,
) -> Operations {
    let mut ops = Vec::new();
    let mut total: i128 = 0;
    for (i, (recipient, amount)) in recipients.iter().enumerate() {
        let amount_i128 = *amount as i128;
        total += amount_i128;
        ops.push(json!({
            "operation_identifier": {"index": i},
            "type": "PayCoin",
            "account": {"address": recipient.to_string()},
            "amount": {
                "value": amount_i128.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {"coin_type": coin_type}
                }
            }
        }));
    }
    ops.push(json!({
        "operation_identifier": {"index": recipients.len()},
        "type": "PayCoin",
        "account": {"address": sender.to_string()},
        "amount": {
            "value": (-total).to_string(),
            "currency": {
                "symbol": "TEST_COIN",
                "decimals": TEST_COIN_DECIMALS,
                "metadata": {"coin_type": coin_type}
            }
        }
    }));
    serde_json::from_value(json!(ops)).unwrap()
}

/// Publish the test custom coin, mint `mint_amount` to `sender` as coin OBJECTS (no address-balance
/// deposit), and register it as a gasless (free-tier) token with `min_transfer`. Returns the coin
/// type's canonical string. `GASLESS_TOKENS_FOR_TESTING` is process-global and additive; package IDs
/// are unique per test so registrations don't collide.
async fn setup_gasless_coin_as_objects(
    test_cluster: &test_cluster::TestCluster,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    mint_amount: u64,
    min_transfer: u64,
) -> String {
    let init_ret = init_package(
        test_cluster,
        client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    mint(
        test_cluster,
        client,
        keystore,
        init_ret,
        vec![(mint_amount, sender)],
    )
    .await
    .unwrap();

    sui_types::transaction::add_gasless_token_for_testing(coin_type.clone(), min_transfer);

    coin_type
}

/// Like `setup_gasless_coin_as_objects`, but additionally moves the whole minted amount into the
/// sender's address balance (so the payment is funded entirely from AB).
async fn setup_gasless_coin_in_ab(
    test_cluster: &test_cluster::TestCluster,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    mint_amount: u64,
    min_transfer: u64,
) -> String {
    let coin_type = setup_gasless_coin_as_objects(
        test_cluster,
        client,
        keystore,
        sender,
        mint_amount,
        min_transfer,
    )
    .await;

    deposit_to_address_balance(
        test_cluster,
        client,
        keystore,
        sender,
        mint_amount,
        Some(&coin_type),
    )
    .await;

    coin_type
}

/// Path A: sender has address balance (from send_funds deposit), so gas comes from AB.
/// Asserts `metadata.gas_coins.is_empty()`.
#[tokio::test]
async fn test_pay_sui_ab_gas_coin_payment() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(60000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Deposit 10 SUI to address balance via coin::send_funds
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        10_000_000_000,
        None,
    )
    .await;

    // Now sender has AB. PaySui should use Path A (AB gas).
    let initial_balance = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    let ops = pay_sui_ops(sender, recipient, "1000000000");

    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Path A: gas_coins should be empty (AB gas), got {:?}",
        metadata.metadata.gas_coins
    );

    // Complete the flow and verify success
    let response: TransactionIdentifierResponse = flow
        .submit
        .expect("Submit was None")
        .expect("Submit failed");
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    let after_balance = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert!(
        after_balance < initial_balance
            && after_balance >= initial_balance - 1_000_000_000 - 50_000_000,
        "Balance should decrease by ~payment + gas. Before: {initial_balance}, after: {after_balance}"
    );
}

/// Path A with deficit: payment exceeds coin balance, so the shortfall is withdrawn from AB.
/// Sender starts with a small coin (~30 SUI) and a larger AB deposit (~100 SUI).
/// PaySui for an amount larger than the coin forces a deficit withdrawal from AB.
#[tokio::test]
async fn test_pay_sui_ab_deficit() {
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![30_000_000_000], // 30 SUI
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(60000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Split 15 SUI to AB, leaving ~15 SUI in coin (minus gas).
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        15_000_000_000,
        None,
    )
    .await;

    // sender now has ~15 SUI in coin (minus gas) and 15 SUI in AB.
    // PaySui 20 SUI: coin (~14.5 SUI) can't cover it, deficit (~5.5 SUI) pulled from AB.
    let initial_balance = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    let payment = 20_000_000_000u64; // 20 SUI
    let ops = pay_sui_ops(sender, recipient, &payment.to_string());

    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Deficit path: gas_coins should be empty (AB gas), got {:?}",
        metadata.metadata.gas_coins
    );

    let response: TransactionIdentifierResponse = flow
        .submit
        .expect("Submit was None")
        .expect("Submit failed");
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    let after_balance = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert!(
        after_balance < initial_balance && after_balance >= initial_balance - payment - 50_000_000,
        "Balance should decrease by ~payment + gas. Before: {initial_balance}, after: {after_balance}"
    );
}

/// Path A with no coin objects: sender's entire SUI balance is in address balance.
/// PaySui draws entirely from AB for both payment and gas.
#[tokio::test]
async fn test_pay_sui_entirely_from_ab() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    sweep_all_coins_to_ab(&test_cluster, &mut client, keystore, sender).await;

    // Verify no coin objects remain
    let coins = client
        .select_up_to_n_largest_coins(
            &Address::from(sender),
            &sui_sdk_types::StructTag::sui().into(),
            10,
            &[],
        )
        .await
        .unwrap();
    assert!(
        coins.is_empty(),
        "Should have zero SUI coin objects after sweep"
    );

    let balance_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert!(
        balance_before > 0,
        "Should still have SUI in address balance"
    );

    let payment = 1_000_000_000u64; // 1 SUI
    let ops = pay_sui_ops(sender, recipient, &payment.to_string());

    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Entirely from AB: gas_coins should be empty, got {:?}",
        metadata.metadata.gas_coins
    );

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
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    let after_balance = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert!(
        after_balance < balance_before && after_balance >= balance_before - payment - 50_000_000,
        "Balance should decrease by ~payment + gas. Before: {balance_before}, after: {after_balance}"
    );
}

/// Path B with AB withdrawal: AB too small for gas (1M mist) so coins provide gas,
/// but Path B still withdraws the full AB and merges it into GasCoin.
/// Verifies `address_balance_withdrawal` in metadata equals the deposited amount.
#[tokio::test]
async fn test_pay_sui_coin_gas_with_ab() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Deposit 1M mist to AB — too small for gas, so Path A fails → Path B.
    let ab_deposit = 1_000_000u64;
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        ab_deposit,
        None,
    )
    .await;

    let initial_balance = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    let ops = pay_sui_ops(sender, recipient, "1000000000");

    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        !metadata.metadata.gas_coins.is_empty(),
        "Path B: gas_coins should be non-empty (coin gas)"
    );
    assert_eq!(
        metadata.metadata.address_balance_withdrawal, ab_deposit,
        "Path B should withdraw the full AB"
    );

    let response: TransactionIdentifierResponse = flow
        .submit
        .expect("Submit was None")
        .expect("Submit failed");
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    let after_balance = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert!(
        after_balance < initial_balance
            && after_balance >= initial_balance - 1_000_000_000 - 50_000_000,
        "Balance should decrease by ~payment + gas. Before: {initial_balance}, after: {after_balance}"
    );
}

/// Stake from address balance: deposit SUI to AB, then stake using the address balance.
#[tokio::test]
async fn test_stake_from_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // Deposit SUI to address balance
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        10_000_000_000,
        None,
    )
    .await;

    let balance_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

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

    let stake_amount = 1_000_000_000u64; // 1 SUI
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": format!("-{stake_amount}")},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();

    let r = rosetta_flow_success(&rosetta_client, &mut client, &stake_ops, keystore).await;
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

    let balance_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    let spent = balance_before - balance_after;
    assert!(
        spent >= stake_amount && spent <= stake_amount + 50_000_000,
        "Should have staked {stake_amount} + gas. Before: {balance_before}, after: {balance_after}, spent: {spent}"
    );
}

/// Stake all from address balance: deposit SUI to AB, then stake all
/// (no explicit amount). The total stake = total_balance - gas_budget.
#[tokio::test]
async fn test_stake_all_from_address_balance() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // Deposit SUI to address balance
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        10_000_000_000,
        None,
    )
    .await;

    let balance_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

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

    // Stake all (no amount specified)
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();

    let r = rosetta_flow_success(&rosetta_client, &mut client, &stake_ops, keystore).await;
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

    // Staked amount should be total_balance - gas, so remaining balance is near zero.
    let balance_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    let staked = balance_before - balance_after;
    assert!(
        staked > balance_before * 99 / 100,
        "Stake all should stake nearly everything (coins + AB). \
         Before: {balance_before}, staked: {staked}, after: {balance_after}"
    );
}

/// Stake with deficit: sender has a small coin that partially covers the stake amount,
/// and the shortfall is withdrawn from address balance.
#[tokio::test]
async fn test_stake_deficit_path() {
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![15_000_000_000], // 15 SUI
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // Deposit 10 SUI to AB, leaving ~5 SUI in coin (minus gas).
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        10_000_000_000,
        None,
    )
    .await;

    let balance_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

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

    // Stake 10 SUI: coin (~5 SUI) can't cover it, deficit pulled from AB.
    let stake_amount = 10_000_000_000u64; // 10 SUI
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": format!("-{stake_amount}")},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();

    let r = rosetta_flow_success(&rosetta_client, &mut client, &stake_ops, keystore).await;
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

    let balance_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    let spent = balance_before - balance_after;
    assert!(
        spent >= stake_amount && spent <= stake_amount + 50_000_000,
        "Should have staked {stake_amount} (from coin + AB deficit) + gas. \
         Before: {balance_before}, after: {balance_after}, spent: {spent}"
    );
}

/// Stake entirely from address balance: sender has zero SUI coin objects,
/// everything is in AB. Stake draws from AB for both the stake amount and gas.
#[tokio::test]
async fn test_stake_entirely_from_ab() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    sweep_all_coins_to_ab(&test_cluster, &mut client, keystore, sender).await;

    // Verify no coin objects remain
    let coins = client
        .select_up_to_n_largest_coins(
            &Address::from(sender),
            &sui_sdk_types::StructTag::sui().into(),
            10,
            &[],
        )
        .await
        .unwrap();
    assert!(
        coins.is_empty(),
        "Should have zero SUI coin objects after sweep"
    );

    let balance_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

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

    let stake_amount = 1_000_000_000u64; // 1 SUI
    let stake_ops: Operations = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "Stake",
        "account": {"address": sender.to_string()},
        "amount": {"value": format!("-{stake_amount}")},
        "metadata": {"Stake": {"validator": validator.to_string()}}
    }]))
    .unwrap();

    let r = rosetta_flow_success(&rosetta_client, &mut client, &stake_ops, keystore).await;

    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        r.transaction_identifier.hash,
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&stake_ops),
        "Stake from AB operations mismatch"
    );

    let balance_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    let spent = balance_before - balance_after;
    assert!(
        spent >= stake_amount && spent <= stake_amount + 50_000_000,
        "Should have staked {stake_amount} + gas. \
         Before: {balance_before}, after: {balance_after}, spent: {spent}"
    );
}

/// PayCoin (non-SUI) with SUI address balance available.
/// 1. Deploy custom coin package + mint coins
/// 2. Deposit SUI to address balance
/// 3. PayCoin: custom coins transferred, SUI address balance for gas
#[tokio::test]
async fn test_pay_coin_from_address_balance() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT], // 2 coins: one for init, one for mint+gas
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
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

    // Step 2: Deposit SUI to address balance
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        10_000_000_000,
        None,
    )
    .await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Step 3: PayCoin — custom coins transferred, SUI address balance available for gas
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
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "PayCoin with SUI AB: gas_coins should be empty (AB gas), got {:?}",
        metadata.metadata.gas_coins
    );
    assert_eq!(
        metadata.metadata.address_balance_withdrawal, 0,
        "PayCoin with full coin coverage: no custom coin AB deficit expected"
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

    let sender_balance = get_total_balance(&mut client, sender, &coin_type).await;
    let recipient_balance = get_total_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        sender_balance, 0,
        "Sender should have 0 custom coins after sending all"
    );
    assert_eq!(
        recipient_balance, total_balance as u64,
        "Recipient should have received all custom coins"
    );
}

/// PayCoin with coins + custom coin AB deficit, SUI AB gas.
/// Sender has 300K in coin objects + 200K in custom coin AB.
/// PayCoin 400K → 300K from coin + 100K deficit from AB.
#[tokio::test]
async fn test_pay_coin_ab_deficit() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    // Deploy custom coin and mint 500K to sender (one object)
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

    mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret,
        vec![(500_000, sender)],
    )
    .await
    .unwrap();

    // Deposit SUI to AB for gas
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        10_000_000_000,
        None,
    )
    .await;

    // Deposit 200K custom coins to AB, leaving 300K in coin objects
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        200_000,
        Some(&coin_type),
    )
    .await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // PayCoin 400K: 300K from coin + 100K deficit from AB
    let ops = pay_coin_ops(sender, recipient, 400_000, &coin_type);
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "AB deficit: gas_coins should be empty (AB gas), got {:?}",
        metadata.metadata.gas_coins
    );
    assert!(
        metadata.metadata.address_balance_withdrawal > 0,
        "AB deficit: address_balance_withdrawal should be > 0"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

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
        "PayCoin AB deficit failed: {:?}",
        tx.effects().status().error()
    );

    let sender_balance = get_total_balance(&mut client, sender, &coin_type).await;
    let recipient_balance = get_total_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        sender_balance, 100_000,
        "Sender should have 500K - 400K = 100K remaining"
    );
    assert_eq!(
        recipient_balance, 400_000,
        "Recipient should have received 400K"
    );
}

/// PayCoin with coins + deficit, SUI coin gas (AB too small for gas).
#[tokio::test]
async fn test_pay_coin_ab_deficit_coin_gas() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    // Deploy custom coin and mint 500K to sender
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

    mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret,
        vec![(500_000, sender)],
    )
    .await
    .unwrap();

    // Deposit 1M mist SUI to AB — too small for gas
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        1_000_000,
        None,
    )
    .await;

    // Deposit 200K custom coins to AB, leaving 300K in coin objects
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        200_000,
        Some(&coin_type),
    )
    .await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // PayCoin 400K: 300K from coin + 100K deficit from AB
    let ops = pay_coin_ops(sender, recipient, 400_000, &coin_type);
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        !metadata.metadata.gas_coins.is_empty(),
        "Coin gas: gas_coins should be non-empty (SUI AB too small)"
    );
    assert!(
        metadata.metadata.address_balance_withdrawal > 0,
        "AB deficit: address_balance_withdrawal should be > 0"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

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
        "PayCoin AB deficit with coin gas failed: {:?}",
        tx.effects().status().error()
    );

    let sender_balance = get_total_balance(&mut client, sender, &coin_type).await;
    let recipient_balance = get_total_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        sender_balance, 100_000,
        "Sender should have 500K - 400K = 100K remaining"
    );
    assert_eq!(
        recipient_balance, 400_000,
        "Recipient should have received 400K"
    );
}

/// PayCoin entirely from non-SUI address balance (no coin objects).
/// All custom coins are deposited to AB, PayCoin draws entirely from AB.
#[tokio::test]
async fn test_pay_coin_entirely_from_ab() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    // Deploy custom coin and mint 500K to sender
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

    mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret,
        vec![(500_000, sender)],
    )
    .await
    .unwrap();

    // Deposit SUI to AB for gas
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        10_000_000_000,
        None,
    )
    .await;

    // Deposit ALL 500K custom coins to AB — no coin objects left
    deposit_to_address_balance(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        500_000,
        Some(&coin_type),
    )
    .await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // PayCoin 400K entirely from custom coin AB
    let ops = pay_coin_ops(sender, recipient, 400_000, &coin_type);
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "Entirely from AB: gas_coins should be empty (AB gas), got {:?}",
        metadata.metadata.gas_coins
    );
    assert_eq!(
        metadata.metadata.address_balance_withdrawal, 400_000,
        "Entirely from AB: address_balance_withdrawal should equal payment amount"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

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
        "PayCoin entirely from AB failed: {:?}",
        tx.effects().status().error()
    );

    let sender_balance = get_total_balance(&mut client, sender, &coin_type).await;
    let recipient_balance = get_total_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        sender_balance, 100_000,
        "Sender should have 500K - 400K = 100K remaining"
    );
    assert_eq!(
        recipient_balance, 400_000,
        "Recipient should have received 400K"
    );
}

/// Single-recipient gasless ("free tier") PayCoin: the custom coin is registered as a gasless
/// token and the sender's whole balance sits in their address balance, so the payment routes
/// through the AB-only gasless PTB at zero gas and credits the recipient's address balance.
#[tokio::test]
async fn test_pay_coin_gasless_single_recipient() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    let pay = 400_000u64;
    let coin_type =
        setup_gasless_coin_in_ab(&test_cluster, &mut client, keystore, sender, 500_000, 0).await;

    // Capture the sender's SUI total *after* setup (the deposit txs already spent SUI gas), so the
    // post-flow comparison isolates the rosetta payment's gas impact.
    let sender_sui_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let ops = pay_coin_ops(sender, recipient, pay, &coin_type);
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "gasless: gas_coins should be empty, got {:?}",
        metadata.metadata.gas_coins
    );
    assert_eq!(
        metadata.metadata.gas_price, 0,
        "gasless: gas_price should be 0"
    );
    assert_eq!(metadata.metadata.budget, 0, "gasless: budget should be 0");
    assert_eq!(
        metadata.metadata.address_balance_withdrawal, pay,
        "gasless: withdrawal should equal the payment"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects", "transaction"]));
    let tx = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(
        tx.effects().status().success(),
        "gasless PayCoin failed: {:?}",
        tx.effects().status().error()
    );
    assert_onchain_gasless(&tx);

    let recipient_ab = get_address_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        recipient_ab, pay,
        "recipient should be credited {pay} to address balance"
    );

    let sender_total = get_total_balance(&mut client, sender, &coin_type).await;
    assert_eq!(
        sender_total,
        500_000 - pay,
        "sender coin balance should be debited by the payment"
    );

    // Strongest gaslessness assertion: the sender's SUI is untouched by the payment.
    let sender_sui_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert_eq!(
        sender_sui_after, sender_sui_before,
        "gasless: sender SUI balance must be unchanged"
    );
}

/// Multi-recipient gasless PayCoin: each recipient is credited its own amount in address balance
/// and the withdrawal equals the sum.
#[tokio::test]
async fn test_pay_coin_gasless_multi_recipient() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient_1 = test_cluster.get_address_1();
    let recipient_2 = test_cluster.get_address_2();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    let pay_1 = 300_000u64;
    let pay_2 = 150_000u64;
    let total = pay_1 + pay_2;
    let coin_type =
        setup_gasless_coin_in_ab(&test_cluster, &mut client, keystore, sender, 500_000, 0).await;

    let sender_sui_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let ops = pay_coin_ops_multi(
        sender,
        &[(recipient_1, pay_1), (recipient_2, pay_2)],
        &coin_type,
    );
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "gasless: gas_coins should be empty"
    );
    assert_eq!(
        metadata.metadata.gas_price, 0,
        "gasless: gas_price should be 0"
    );
    assert_eq!(metadata.metadata.budget, 0, "gasless: budget should be 0");
    assert_eq!(
        metadata.metadata.address_balance_withdrawal, total,
        "gasless: withdrawal should equal the sum of payments"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));
    let tx = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(
        tx.effects().status().success(),
        "gasless multi-recipient PayCoin failed: {:?}",
        tx.effects().status().error()
    );

    let recipient_1_ab = get_address_balance(&mut client, recipient_1, &coin_type).await;
    let recipient_2_ab = get_address_balance(&mut client, recipient_2, &coin_type).await;
    assert_eq!(recipient_1_ab, pay_1, "recipient_1 AB should equal pay_1");
    assert_eq!(recipient_2_ab, pay_2, "recipient_2 AB should equal pay_2");

    let sender_total = get_total_balance(&mut client, sender, &coin_type).await;
    assert_eq!(
        sender_total,
        500_000 - total,
        "sender coin balance should be debited by the total"
    );

    let sender_sui_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert_eq!(
        sender_sui_after, sender_sui_before,
        "gasless: sender SUI balance must be unchanged"
    );
}

/// Below-min fallback: the coin is registered with a per-token minimum the payment doesn't meet,
/// so the node's gasless auto-switch declines and the payment falls back to the priced smash path.
/// The recipient receives a coin object (address balance stays 0), and a gas price is charged.
#[tokio::test]
async fn test_pay_coin_gasless_below_min_falls_back() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    let pay = 5_000u64;
    // Register with a minimum the 5_000 payment can't meet → gasless declines, priced fallback.
    let coin_type = setup_gasless_coin_in_ab(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        500_000,
        10_000,
    )
    .await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let ops = pay_coin_ops(sender, recipient, pay, &coin_type);
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_price > 0,
        "below-min: should use the priced smash path (gas_price > 0)"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));
    let tx = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(
        tx.effects().status().success(),
        "below-min smash PayCoin failed: {:?}",
        tx.effects().status().error()
    );

    // Recipient got a coin object via the smash path, not an address-balance credit.
    let recipient_ab = get_address_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        recipient_ab, 0,
        "below-min: recipient should receive a coin object, not an AB credit"
    );
    let recipient_total = get_total_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        recipient_total, pay,
        "below-min: recipient total should equal the payment"
    );
}

/// A client-supplied gas budget must NOT make a free-tier payment ineligible — budget is a ceiling
/// for the priced path, and the gasless path ignores it (and the node force-zeroes budget for the
/// gasless variant). Same as the single-recipient success case, but the request carries an explicit
/// non-zero budget; the result must still be gasless (price 0, budget 0, zero on-chain gas).
#[tokio::test]
async fn test_pay_coin_gasless_ignores_request_budget() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    let pay = 400_000u64;
    let coin_type =
        setup_gasless_coin_in_ab(&test_cluster, &mut client, keystore, sender, 500_000, 0).await;

    let sender_sui_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let ops = pay_coin_ops(sender, recipient, pay, &coin_type);
    // Explicit non-zero request budget — should be ignored for the free tier.
    let flow = rosetta_client
        .rosetta_flow(
            &ops,
            keystore,
            Some(PreprocessMetadata {
                budget: Some(50_000_000),
            }),
        )
        .await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "gasless: gas_coins should be empty despite a request budget"
    );
    assert_eq!(
        metadata.metadata.gas_price, 0,
        "gasless: gas_price should be 0 despite a request budget"
    );
    assert_eq!(
        metadata.metadata.budget, 0,
        "gasless: response budget should be 0, not the requested 50_000_000"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects", "transaction"]));
    let tx = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(
        tx.effects().status().success(),
        "gasless PayCoin (with request budget) failed: {:?}",
        tx.effects().status().error()
    );

    // Still actually free despite the non-zero request budget: on-chain price/budget 0, no gas coins.
    assert_onchain_gasless(&tx);
    let recipient_ab = get_address_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        recipient_ab, pay,
        "recipient should be credited {pay} to AB"
    );

    let sender_sui_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert_eq!(
        sender_sui_after, sender_sui_before,
        "gasless: sender SUI must be unchanged despite a request budget"
    );
}

/// Free-tier payment funded from coin OBJECTS (not address balance). The coins are smashed and
/// consumed gaslessly: recipients are credited to their address balance and the change lands in the
/// sender's address balance. Proves the gasless path no longer requires funds to already be in AB.
#[tokio::test]
async fn test_pay_coin_gasless_from_coin_objects() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    let pay = 400_000u64;
    // Mint 500K as coin OBJECTS — left in the sender's wallet, NOT deposited to AB.
    let coin_type =
        setup_gasless_coin_as_objects(&test_cluster, &mut client, keystore, sender, 500_000, 0)
            .await;

    let sender_sui_before = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let ops = pay_coin_ops(sender, recipient, pay, &coin_type);
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_coins.is_empty(),
        "gasless: gas_coins should be empty"
    );
    assert_eq!(
        metadata.metadata.gas_price, 0,
        "gasless: gas_price should be 0"
    );
    assert_eq!(metadata.metadata.budget, 0, "gasless: budget should be 0");
    assert_eq!(
        metadata.metadata.address_balance_withdrawal, 0,
        "coin-funded gasless: nothing withdrawn from AB (funded entirely from coins)"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects", "transaction"]));
    let tx = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(
        tx.effects().status().success(),
        "coin-funded gasless PayCoin failed: {:?}",
        tx.effects().status().error()
    );
    assert_onchain_gasless(&tx);

    // Recipient credited to AB; change (500K - 400K) lands in the SENDER's AB.
    let recipient_ab = get_address_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        recipient_ab, pay,
        "recipient should be credited {pay} to AB"
    );

    let sender_ab = get_address_balance(&mut client, sender, &coin_type).await;
    assert_eq!(
        sender_ab,
        500_000 - pay,
        "sender's change should land in their address balance"
    );
    let sender_total = get_total_balance(&mut client, sender, &coin_type).await;
    assert_eq!(
        sender_total,
        500_000 - pay,
        "sender's coin objects are consumed; only the AB change remains"
    );

    let sender_sui_after = get_total_balance(&mut client, sender, SUI_COIN_TYPE).await;
    assert_eq!(
        sender_sui_after, sender_sui_before,
        "gasless: sender SUI must be unchanged"
    );
}

/// Coin-funded free tier where the *change* would fall below the per-token minimum. The protocol
/// rejects the gasless variant (sub-min change deposited back to the sender), so rosetta falls back
/// to the priced smash path and the recipient receives a coin object.
#[tokio::test]
async fn test_pay_coin_gasless_change_below_min_falls_back() {
    const AMOUNT: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT, AMOUNT],
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(accounts)
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    let pay = 400_000u64;
    // Coins total 405K, min 10K. Paying 400K leaves 5K change to the sender's AB — below the 10K
    // minimum — so the node declines gasless and rosetta falls back to smash.
    let coin_type = setup_gasless_coin_as_objects(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        405_000,
        10_000,
    )
    .await;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let ops = pay_coin_ops(sender, recipient, pay, &coin_type);
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    let metadata = flow
        .metadata
        .as_ref()
        .unwrap()
        .as_ref()
        .expect("Metadata failed");
    assert!(
        metadata.metadata.gas_price > 0,
        "sub-min change: should fall back to the priced smash path (gas_price > 0)"
    );

    if let Some(Err(e)) = &flow.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow.combine {
        panic!("Combine failed: {:?}", e);
    }

    let r: TransactionIdentifierResponse = flow.submit.unwrap().unwrap();
    wait_for_transaction(&mut client, &r.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(r.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));
    let tx = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();
    assert!(
        tx.effects().status().success(),
        "smash fallback PayCoin failed: {:?}",
        tx.effects().status().error()
    );

    // Recipient got a coin object via the smash path, not an AB credit.
    let recipient_ab = get_address_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        recipient_ab, 0,
        "sub-min change fallback: recipient should receive a coin object, not an AB credit"
    );
    let recipient_total = get_total_balance(&mut client, recipient, &coin_type).await;
    assert_eq!(
        recipient_total, pay,
        "sub-min change fallback: recipient total should equal the payment"
    );
}

/// Test that address-balance gas payments (where effects.gas_object is None) are correctly
/// parsed into Operations.
#[tokio::test]
async fn test_address_balance_gas_payment_parsing() {
    use std::collections::BTreeMap;
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

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
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
        vec![],                             // unchanged_consensus_objects
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

/// End-to-end regression for the Coinbase free-tier crash (PR #26756): a
/// `coin::send_funds<SUI>` that moves the *entire gas coin* into an address
/// balance deletes the gas object, so its effects carry no output owner.
/// `try_from_executed_transaction` previously fed that empty owner string to
/// `SuiAddress::from_str`, producing `FastCryptoError::InvalidInput`
/// ("Invalid value was given to the function") and failing the whole `/block`
/// request. It must instead parse, attributing gas to the gas payment owner.
#[tokio::test]
async fn test_send_gas_coin_to_address_balance_parses() {
    use sui_rosetta::types::OperationType;
    use sui_types::transaction::Argument;

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_accounts(single_coin_accounts())
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    // Send the entire gas coin into the sender's own address balance. The account has
    // a single coin, so it is both the gas coin and the funds being sent: it gets
    // consumed (deleted), leaving effects.gas_object with no output owner — the exact
    // shape from the Coinbase failure.
    let mut ptb = ProgrammableTransactionBuilder::new();
    let recipient_arg = ptb.pure(sender).unwrap();
    ptb.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::from(sui_types::coin::COIN_MODULE_NAME),
        Identifier::new("send_funds").unwrap(),
        vec![sui_types::gas_coin::GAS::type_tag()],
        vec![Argument::GasCoin, recipient_arg],
    ));
    let pt = ptb.finish();

    let price = client.get_reference_gas_price().await.unwrap();
    let budget = 500_000_000u64;
    let (_, gas_object_data) = test_cluster
        .wallet
        .gas_for_owner_budget(sender, budget, BTreeSet::new())
        .await
        .unwrap();
    let gas_object = gas_object_data.compute_object_reference();
    let tx_data = TransactionData::new_programmable(sender, vec![gas_object], pt, budget, price);
    let sig = keystore
        .sign_secure(
            &tx_data.sender(),
            &tx_data,
            shared_crypto::intent::Intent::sui_transaction(),
        )
        .await
        .unwrap();
    let signed_tx = sui_types::transaction::Transaction::from_data(tx_data, vec![sig]);
    let response = test_utils::execute_transaction(&mut client, &signed_tx)
        .await
        .unwrap();
    assert!(
        response.effects().status().success(),
        "send_funds(GasCoin) into address balance failed: {:?}",
        response.effects().status().error()
    );

    // Precondition: the gas coin was deleted, so its effects carry no output owner.
    // Assert it so the test fails loudly if execution ever stops producing this shape.
    assert!(
        response
            .effects()
            .gas_object()
            .output_owner()
            .address()
            .is_empty(),
        "expected a deleted gas object with no output owner; test no longer covers the regression"
    );

    // The fix: parsing succeeds instead of erroring with InvalidInput.
    let ops = fetch_transaction_and_get_operations(&test_cluster, *signed_tx.digest(), &coin_cache)
        .await
        .expect("rosetta failed to parse send_funds(GasCoin) transaction");

    // Gas must be attributed to the gas payment owner (the sender) via the fallback.
    let gas_op = ops
        .into_iter()
        .find(|op| op.type_ == OperationType::Gas)
        .expect("expected a Gas operation");
    assert_eq!(
        gas_op.account.map(|a| a.address),
        Some(sender),
        "Gas should be attributed to the gas payment owner"
    );
}
