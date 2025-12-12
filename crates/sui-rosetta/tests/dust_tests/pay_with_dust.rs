// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use anyhow::{Result, anyhow};
use once_cell::sync::Lazy;
use prost_types::FieldMask;
use serde_json::json;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::PreprocessMetadata;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetObjectsRequest, GetObjectRequest, GetTransactionRequest,
};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::supported_protocol_versions::ProtocolConfig;
use sui_types::transaction::{Transaction, TransactionData};
use test_cluster::TestClusterBuilder;

use super::rosetta_client::{RosettaError, start_rosetta_test_server};
use super::split_coin::{DEFAULT_GAS_BUDGET, make_change};
use crate::test_utils::{
    execute_transaction, get_all_coins, get_coin_value, get_object_ref, wait_for_transaction,
};

static MAX_GAS_BUDGET: Lazy<u64> =
    Lazy::new(|| ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas());

#[tokio::test]
async fn test_pay_with_many_small_coins() -> Result<()> {
    use sui_swarm_config::genesis_config::AccountConfig;

    // Use 150K SUI to ensure we stay under the 1M SUI mock gas limit
    const AMOUNT_150K_SUI: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT_150K_SUI, AMOUNT_150K_SUI], // Two gas objects per account
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .with_accounts(accounts)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;

    let first_coin_id = all_coins_sender[0].id();
    let second_coin_id = all_coins_sender[1].id();

    let gas_price = client.get_reference_gas_price().await?;

    let mut gas_for_transfers = all_coins_sender[0].compute_object_reference();

    for coin in all_coins_sender.iter().skip(2) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        ptb.transfer_object(recipient, coin.compute_full_object_reference())?;
        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_for_transfers],
            ptb.finish(),
            DEFAULT_GAS_BUDGET,
            gas_price,
        );
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
        let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
        let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        gas_for_transfers = get_object_ref(&mut client.clone(), all_coins_sender[0].id())
            .await?
            .as_object_ref();
    }

    let mut first_coin_request = GetObjectRequest::default();
    first_coin_request.object_id = Some(first_coin_id.to_string());

    let mut second_coin_request = GetObjectRequest::default();
    second_coin_request.object_id = Some(second_coin_id.to_string());

    let mut batch_request = BatchGetObjectsRequest::default();
    batch_request.requests = vec![first_coin_request, second_coin_request];
    batch_request.read_mask = Some(FieldMask::from_paths(["bcs"]));

    let batch_response = client
        .ledger_client()
        .batch_get_objects(batch_request)
        .await?
        .into_inner();

    let objects = batch_response.objects;
    if objects.len() != 2 {
        return Err(anyhow!("Expected 2 coins, got {}", objects.len()));
    }

    let first_coin_obj = objects[0].object();
    let second_coin_obj = objects[1].object();

    let coin_to_split = first_coin_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;
    let gas_for_split_tx = second_coin_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;
    let new_coins = 300;
    let split_amount = get_coin_value(&coin_to_split) / new_coins;
    let amount_to_send = split_amount as i128 * 257;
    let recipient_change = amount_to_send.to_string();
    let sender_change = (-amount_to_send).to_string();

    // Split balance to something that will need more than 255 coins to execute:
    let _resps = make_change(
        &mut client.clone(),
        keystore,
        sender,
        &coin_to_split,
        Some(gas_for_split_tx.compute_object_reference()),
        split_amount,
    )
    .await?;

    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await?
        .unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.transfer_object(
        recipient,
        get_object_ref(&mut client.clone(), gas_for_split_tx.id()).await?,
    )?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": recipient_change }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": sender_change }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch transaction using gRPC
    let mut grpc_request = GetTransactionRequest::default();
    grpc_request.digest = Some(response.transaction_identifier.hash.to_string());
    grpc_request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction",
        "effects",
        "balance_changes",
        "events",
    ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );
    // Create a gRPC client and fetch the transaction with gRPC
    let client_copy = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let tx_digest = tx.digest;

    let mut grpc_request = GetTransactionRequest::default();
    grpc_request.digest = tx_digest.clone();
    grpc_request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction",
        "effects",
        "balance_changes",
        "events.events.event_type",
        "events.events.json",
        "events.events.contents",
    ]));

    let mut client_mut = client_copy.clone();
    let grpc_response = client_mut
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let coin_cache = CoinMetadataCache::new(client_copy.clone(), NonZeroUsize::new(2).unwrap());
    let executed_tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");
    let ops2 = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string_pretty(&ops).unwrap(),
        serde_json::to_string_pretty(&ops2).unwrap()
    );

    Ok(())
}

// The limit actually passes for 1650 coins, but it often fails with
// "Failed to confirm tx status for TransactionDigest(...) within .. seconds.".
// So we use a smaller split-count in the test, but it can pass with a larger amount locally.
// This originates from the fact that we pass None as the ExecuteTransactionRequestType
// in the submit endpoint. This defaults to WaitForLocalExecution which has a timetout.
#[tokio::test]
async fn test_limit_many_small_coins() -> Result<()> {
    use sui_swarm_config::genesis_config::AccountConfig;

    // Use 150K SUI to ensure we stay under the 1M SUI mock gas limit
    const AMOUNT_150K_SUI: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT_150K_SUI, AMOUNT_150K_SUI], // Two gas objects per account
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .with_accounts(accounts)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;

    let first_coin_id = all_coins_sender[0].id();
    let second_coin_id = all_coins_sender[1].id();

    let gas_price = client.get_reference_gas_price().await?;

    let mut gas_for_transfers = all_coins_sender[0].compute_object_reference();

    for coin in all_coins_sender.iter().skip(2) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        ptb.transfer_object(recipient, coin.compute_full_object_reference())?;
        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_for_transfers],
            ptb.finish(),
            DEFAULT_GAS_BUDGET,
            gas_price,
        );
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
        let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
        let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        gas_for_transfers = get_object_ref(&mut client.clone(), all_coins_sender[0].id())
            .await?
            .as_object_ref();
    }

    let mut first_coin_request = GetObjectRequest::default();
    first_coin_request.object_id = Some(first_coin_id.to_string());

    let mut second_coin_request = GetObjectRequest::default();
    second_coin_request.object_id = Some(second_coin_id.to_string());

    let mut batch_request = BatchGetObjectsRequest::default();
    batch_request.requests = vec![first_coin_request, second_coin_request];
    batch_request.read_mask = Some(FieldMask::from_paths(["bcs"]));

    let batch_response = client
        .ledger_client()
        .batch_get_objects(batch_request)
        .await?
        .into_inner();

    let objects = batch_response.objects;
    if objects.len() != 2 {
        return Err(anyhow!("Expected 2 coins, got {}", objects.len()));
    }

    let first_coin_obj = objects[0].object();
    let second_coin_obj = objects[1].object();

    let coin_to_split = first_coin_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;
    let gas_for_split_tx = second_coin_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;
    // To test the 1500 coin limit, we need to create MORE than 1500 actual coins
    // But not too many or the test will take forever. Target ~2000 coins.
    // With 150K SUI, we want split_amount that gives us ~2000 coins
    let target_coins = 2000u64;
    let split_amount = get_coin_value(&coin_to_split) / (target_coins + 1);

    // Request payment that uses most of the 1500 coin limit but leaves room for gas
    // We have ~2000 coins total, rosetta will select up to 1500
    // Send amount for ~1400 coins to leave buffer for gas
    let amount_to_send = split_amount as i128 * 1400;
    let recipient_change = amount_to_send.to_string();
    let sender_change = (-amount_to_send).to_string();

    // Split balance to something that will need more than 255 coins to execute:
    let _resps = make_change(
        &mut client.clone(),
        keystore,
        sender,
        &coin_to_split,
        Some(gas_for_split_tx.compute_object_reference()),
        split_amount,
    )
    .await?;

    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await?
        .unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.transfer_object(
        recipient,
        get_object_ref(&mut client.clone(), gas_for_split_tx.id()).await?,
    )?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": recipient_change }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": sender_change }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch transaction using gRPC
    let mut grpc_request = GetTransactionRequest::default();
    grpc_request.digest = Some(response.transaction_identifier.hash.to_string());
    grpc_request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction",
        "effects",
        "balance_changes",
        "events",
    ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );
    // Create a gRPC client and fetch the transaction with gRPC
    let client_copy = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let tx_digest = tx.digest;

    let mut grpc_request = GetTransactionRequest::default();
    grpc_request.digest = tx_digest.clone();
    grpc_request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction",
        "effects",
        "balance_changes",
        "events.events.event_type",
        "events.events.json",
        "events.events.contents",
    ]));

    let mut client_mut = client_copy.clone();
    let grpc_response = client_mut
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let coin_cache = CoinMetadataCache::new(client_copy.clone(), NonZeroUsize::new(2).unwrap());
    let executed_tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");
    let ops2 = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string_pretty(&ops).unwrap(),
        serde_json::to_string_pretty(&ops2).unwrap()
    );

    Ok(())
}

#[tokio::test]
async fn test_pay_with_many_small_coins_with_budget() -> Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;

    let first_coin_id = all_coins_sender[0].id();
    let second_coin_id = all_coins_sender[1].id();

    let gas_price = client.get_reference_gas_price().await?;

    // Send rest of the coins to recipient first
    for coin in all_coins_sender.iter().skip(2) {
        // Get fresh gas object for each transaction
        let gas_object = test_cluster
            .wallet
            .get_one_gas_object_owned_by_address(sender)
            .await?
            .unwrap();

        let mut ptb = ProgrammableTransactionBuilder::new();
        ptb.transfer_object(recipient, coin.compute_full_object_reference())?;
        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_object],
            ptb.finish(),
            DEFAULT_GAS_BUDGET,
            gas_price,
        );
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
        let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
        let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;
    }

    let mut first_coin_request = GetObjectRequest::default();
    first_coin_request.object_id = Some(first_coin_id.to_string());

    let mut second_coin_request = GetObjectRequest::default();
    second_coin_request.object_id = Some(second_coin_id.to_string());

    let mut batch_request = BatchGetObjectsRequest::default();
    batch_request.requests = vec![first_coin_request, second_coin_request];
    batch_request.read_mask = Some(FieldMask::from_paths(["bcs"]));

    let batch_response = client
        .ledger_client()
        .batch_get_objects(batch_request)
        .await?
        .into_inner();

    let objects = batch_response.objects;
    if objects.len() != 2 {
        return Err(anyhow!("Expected 2 coins, got {}", objects.len()));
    }

    let first_coin_obj = objects[0].object();
    let second_coin_obj = objects[1].object();

    let coin_to_split = first_coin_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;
    let gas_for_split_tx = second_coin_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;
    let new_coins = 300;
    let split_amount = get_coin_value(&coin_to_split) / new_coins;
    let amount_to_send = split_amount as i128 * 257;
    let budget = u64::min(split_amount * (new_coins - 258), *MAX_GAS_BUDGET);
    let recipient_change = amount_to_send.to_string();
    let sender_change = (-amount_to_send).to_string();

    // Split balance to something that will need more than 255 coins to execute:
    let _resps = make_change(
        &mut client.clone(),
        keystore,
        sender,
        &coin_to_split,
        Some(gas_for_split_tx.compute_object_reference()),
        split_amount,
    )
    .await?;

    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await?
        .unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.transfer_object(
        recipient,
        get_object_ref(&mut client.clone(), gas_for_split_tx.id()).await?,
    )?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": recipient_change }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": sender_change }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(
            &ops,
            keystore,
            Some(PreprocessMetadata {
                budget: Some(budget),
            }),
        )
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch transaction using gRPC
    let mut grpc_request = GetTransactionRequest::default();
    grpc_request.digest = Some(response.transaction_identifier.hash.to_string());
    grpc_request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction",
        "effects",
        "balance_changes",
        "events",
    ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );

    let tx_digest = tx.digest;
    let mut grpc_request = GetTransactionRequest::default();
    grpc_request.digest = tx_digest.clone();
    grpc_request.read_mask = Some(FieldMask::from_paths([
        "digest",
        "transaction",
        "effects",
        "balance_changes",
        "events.events.event_type",
        "events.events.json",
        "events.events.contents",
    ]));

    let mut client_mut = client.clone();
    let grpc_response = client_mut
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let executed_tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");
    let ops2 = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string_pretty(&ops).unwrap(),
        serde_json::to_string_pretty(&ops2).unwrap()
    );

    Ok(())
}

#[tokio::test]
async fn test_pay_with_many_small_coins_fail_insufficient_balance_budget_none() -> Result<()> {
    use sui_swarm_config::genesis_config::AccountConfig;

    // Use 150K SUI to ensure we stay under the 1M SUI mock gas limit
    const AMOUNT_150K_SUI: u64 = 150_000_000_000_000;
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT_150K_SUI, AMOUNT_150K_SUI], // Two gas objects per account
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .with_accounts(accounts)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // Get all owned objects for sender and filter for coins
    let all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;
    // Note: gRPC implementation handles pagination internally, so no need to check has_next_page

    let gas_price = client.get_reference_gas_price().await?;

    let mut gas_for_transfers = all_coins_sender[0].compute_object_reference();

    for coin in all_coins_sender.iter().skip(2) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        ptb.transfer_object(recipient, coin.compute_full_object_reference())?;
        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_for_transfers],
            ptb.finish(),
            DEFAULT_GAS_BUDGET,
            gas_price,
        );
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
        let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
        let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        gas_for_transfers = get_object_ref(&mut client.clone(), all_coins_sender[0].id())
            .await?
            .as_object_ref();
    }

    // First coin was used for gas, so update coins:
    let mut all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;
    assert!(
        all_coins_sender.len() == 2,
        "Should have exactly 2 coins by now."
    );

    // Keep one small coin
    let coin_to_send = all_coins_sender.pop().unwrap();
    let new_coins = 300;
    let split_amount = 50_000_000;
    assert!(
        new_coins > 255,
        "Test requires more than 255 coins to test multiple merge commands"
    );
    let send_amount_to_decrease_balance = get_coin_value(&coin_to_send) - new_coins * split_amount;

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.pay_sui(vec![recipient], vec![send_amount_to_decrease_balance])?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![coin_to_send.compute_object_reference()], // Use the coin as gas
        ptb.finish(),
        10_000_000,
        gas_price,
    );

    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;
    // Check that the transaction was successful
    let effects = resp.effects();
    if !effects.status().success() {
        let error = effects.status().error_opt();
        panic!("Transaction failed: {:?}", error);
    }

    // Update the coins state
    let mut all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;

    // Differentiate between the two coins
    let coin_to_split_id = coin_to_send.id().into();
    let (coin_to_split, gas_for_split_tx) = match all_coins_sender.as_mut_slice() {
        [coin_0, _] if *coin_0.id() == coin_to_split_id => {
            let gas_for_split_tx = all_coins_sender.pop().unwrap();
            (all_coins_sender.pop().unwrap(), gas_for_split_tx)
        }
        [_, _] => (
            all_coins_sender.pop().unwrap(),
            all_coins_sender.pop().unwrap(),
        ),
        _ => unreachable!("Vector should have exactly two elements"),
    };
    let initial_balance = get_coin_value(&coin_to_split);

    // Split balance to something that will need more than 255 coins to execute:
    let resps = make_change(
        &mut client.clone(),
        keystore,
        sender,
        &coin_to_split,
        Some(gas_for_split_tx.compute_object_reference()),
        split_amount,
    )
    .await?;

    for resp in resps {
        assert!(
            resp.effects().status().success(),
            "Something went wrong splitting coins to change"
        );

        // Wait for each make_change transaction to be indexed
        if let Some(digest) = resp.digest_opt() {
            wait_for_transaction(&mut client, digest).await.unwrap();
        }
    }

    // Now send coin previously been used as gas, in order to only have
    // the change coins.
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await?
        .unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.transfer_object(
        recipient,
        get_object_ref(&mut client.clone(), gas_for_split_tx.id()).await?,
    )?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        10_000_000,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;
    let effects_bcs = resp.effects().bcs();
    let effects: sui_types::effects::TransactionEffects = effects_bcs.deserialize().unwrap();
    let tx_cost_summary = effects.gas_cost_summary().net_gas_usage();
    let total_amount = initial_balance as i128 - tx_cost_summary as i128;
    let expected_budget = 2_100_000; // Calculated after a successful dry-run
    let recipient_change = total_amount - expected_budget + 1; // Make it fail with insufficient
    let sender_change = expected_budget - total_amount - 1;

    // Test rosetta can handle using many "small" coins for payment
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": recipient_change.to_string() }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": sender_change.to_string() }
        }]
    ))
    .unwrap();

    let resps = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    let Some(Err(err)) = resps.metadata else {
        panic!("Expected metadata to exists and error");
    };

    let details = Some(
        json!({ "error": "ExecutionError: Kind: INSUFFICIENT_COIN_BALANCE, Description: InsufficientCoinBalance in command 1" }),
    );
    assert_eq!(
        err,
        RosettaError {
            code: 11,
            message: "Transaction dry run error".to_string(),
            description: None,
            retriable: false,
            details
        }
    );

    Ok(())
}

#[tokio::test]
async fn test_pay_with_many_small_coins_fail_insufficient_balance_with_budget() -> Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // Get all owned objects for sender and filter for coins
    let all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;
    // Note: gRPC implementation handles pagination internally, so no need to check has_next_page

    let gas_price = client.get_reference_gas_price().await?;

    let mut gas_for_transfers = all_coins_sender[0].compute_object_reference();

    for coin in all_coins_sender.iter().skip(2) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        ptb.transfer_object(recipient, coin.compute_full_object_reference())?;
        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_for_transfers],
            ptb.finish(),
            DEFAULT_GAS_BUDGET,
            gas_price,
        );
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
        let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
        let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        gas_for_transfers = get_object_ref(&mut client.clone(), all_coins_sender[0].id())
            .await?
            .as_object_ref();
    }

    // First coin was used for gas, so update coins:
    let mut all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;
    assert!(
        all_coins_sender.len() == 2,
        "Should have exactly 2 coins by now."
    );

    // Keep one small coin
    let coin_to_send = all_coins_sender.pop().unwrap();
    let new_coins = 300;
    let split_amount = 50_000_000;
    assert!(
        new_coins > 255,
        "Test requires more than 255 coins to test multiple merge commands"
    );
    let send_amount_to_decrease_balance = get_coin_value(&coin_to_send) - new_coins * split_amount;

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.pay_sui(vec![recipient], vec![send_amount_to_decrease_balance])?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![coin_to_send.compute_object_reference()], // Use the coin as gas
        ptb.finish(),
        10_000_000,
        gas_price,
    );

    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;
    // Check that the transaction was successful
    let effects = resp.effects();
    if !effects.status().success() {
        let error = effects.status().error_opt();
        panic!("Transaction failed: {:?}", error);
    }

    // Update the coins state
    let mut all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;

    // Differentiate between the two coins
    let coin_to_split_id = coin_to_send.id().into();
    let (coin_to_split, gas_for_split_tx) = match all_coins_sender.as_mut_slice() {
        [coin_0, _] if *coin_0.id() == coin_to_split_id => {
            let gas_for_split_tx = all_coins_sender.pop().unwrap();
            (all_coins_sender.pop().unwrap(), gas_for_split_tx)
        }
        [_, _] => (
            all_coins_sender.pop().unwrap(),
            all_coins_sender.pop().unwrap(),
        ),
        _ => unreachable!("Vector should have exactly two elements"),
    };
    let initial_balance = get_coin_value(&coin_to_split);

    // Split balance to something that will need more than 255 coins to execute:
    let _resps = make_change(
        &mut client.clone(),
        keystore,
        sender,
        &coin_to_split,
        Some(gas_for_split_tx.compute_object_reference()),
        split_amount,
    )
    .await?;

    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await?
        .unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.transfer_object(
        recipient,
        get_object_ref(&mut client.clone(), gas_for_split_tx.id()).await?,
    )?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        10_000_000,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;
    let effects_bcs = resp.effects().bcs();
    let effects: sui_types::effects::TransactionEffects = effects_bcs.deserialize().unwrap();
    let tx_cost_summary = effects.gas_cost_summary().net_gas_usage();
    let total_amount = initial_balance as i128 - tx_cost_summary as i128;
    let budget = 3_076_000; // Calculated from successful dry-run
    let recipient_change = total_amount - budget;
    let sender_change = budget - total_amount;

    // Test rosetta can handle using many "small" coins for payment
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": recipient_change.to_string() }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": sender_change.to_string() }
        }]
    ))
    .unwrap();

    let resps = rosetta_client
        .rosetta_flow(
            &ops,
            keystore,
            Some(PreprocessMetadata {
                budget: Some(budget as u64 + 1), // add 1 to fail
            }),
        )
        .await;

    let details = Some(json!({
        "error": "ExecutionError: Kind: INSUFFICIENT_COIN_BALANCE, Description: InsufficientCoinBalance in command 1"
    }));
    let Some(Err(e)) = resps.metadata else {
        panic!("Expected metadata to exist and error")
    };
    assert_eq!(
        e,
        RosettaError {
            code: 11,
            message: "Transaction dry run error".to_string(),
            description: None,
            retriable: false,
            details,
        },
    );

    Ok(())
}

#[tokio::test]
async fn test_pay_with_many_small_coins_fail_insufficient_budget() -> Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // Get all owned objects for sender and filter for coins
    let all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;
    // Note: gRPC implementation handles pagination internally, so no need to check has_next_page

    let gas_price = client.get_reference_gas_price().await?;

    let mut gas_for_transfers = all_coins_sender[0].compute_object_reference();

    for coin in all_coins_sender.iter().skip(2) {
        let mut ptb = ProgrammableTransactionBuilder::new();
        ptb.transfer_object(recipient, coin.compute_full_object_reference())?;
        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_for_transfers],
            ptb.finish(),
            DEFAULT_GAS_BUDGET,
            gas_price,
        );
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
        let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
        let _resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        gas_for_transfers = get_object_ref(&mut client.clone(), all_coins_sender[0].id())
            .await?
            .as_object_ref();
    }

    // First coin was used for gas, so update coins:
    let mut all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;
    assert!(
        all_coins_sender.len() == 2,
        "Should have exactly 2 coins by now."
    );

    // Keep one small coin
    let coin_to_send = all_coins_sender.pop().unwrap();
    let new_coins = 300;
    let split_amount = 50_000_000;
    assert!(
        new_coins > 255,
        "Test requires more than 255 coins to test multiple merge commands"
    );
    let send_amount_to_decrease_balance = get_coin_value(&coin_to_send) - new_coins * split_amount;

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.pay_sui(vec![recipient], vec![send_amount_to_decrease_balance])?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![coin_to_send.compute_object_reference()], // Use the coin as gas
        ptb.finish(),
        10_000_000,
        gas_price,
    );

    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;
    // Check that the transaction was successful
    let effects = resp.effects();
    if !effects.status().success() {
        let error = effects.status().error_opt();
        panic!("Transaction failed: {:?}", error);
    }

    // Update the coins state
    let mut all_coins_sender = get_all_coins(&mut client.clone(), sender).await?;

    // Differentiate between the two coins
    let coin_to_split_id = coin_to_send.id().into();
    let (coin_to_split, gas_for_split_tx) = match all_coins_sender.as_mut_slice() {
        [coin_0, _] if *coin_0.id() == coin_to_split_id => {
            let gas_for_split_tx = all_coins_sender.pop().unwrap();
            (all_coins_sender.pop().unwrap(), gas_for_split_tx)
        }
        [_, _] => (
            all_coins_sender.pop().unwrap(),
            all_coins_sender.pop().unwrap(),
        ),
        _ => unreachable!("Vector should have exactly two elements"),
    };
    let initial_balance = get_coin_value(&coin_to_split);

    // Split balance to something that will need more than 255 coins to execute:
    let _resps = make_change(
        &mut client.clone(),
        keystore,
        sender,
        &coin_to_split,
        Some(gas_for_split_tx.compute_object_reference()),
        split_amount,
    )
    .await?;

    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await?
        .unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    ptb.transfer_object(
        recipient,
        get_object_ref(&mut client.clone(), gas_for_split_tx.id()).await?,
    )?;
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        10_000_000,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;
    let effects_bcs = resp.effects().bcs();
    let effects: sui_types::effects::TransactionEffects = effects_bcs.deserialize().unwrap();
    let tx_cost_summary = effects.gas_cost_summary().net_gas_usage();
    let total_amount = initial_balance as i128 - tx_cost_summary as i128;

    // Actually budget needed is somewhere between
    // - computation_cost
    // - computation_cost + storage_cost
    // even if rebate is larger than storage_cost.
    // When dry-running to calculate budget, we use computation_cost + storage_cost to be on the safe side,
    // but when using an explicit budget for the transaction, this is skipped and less budget can
    // lead to a succesfull tx.
    let budget = 1_100_000; // This is exactly computation_cost
    let recipient_change = total_amount - budget;
    let sender_change = budget - total_amount;

    // Test rosetta can handle using many "small" coins for payment
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": recipient_change.to_string() }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": sender_change.to_string() }
        }]
    ))
    .unwrap();

    let rosetta_resp = rosetta_client
        .rosetta_flow(
            &ops,
            keystore,
            Some(PreprocessMetadata {
                budget: Some(budget as u64),
            }),
        )
        .await;

    let Some(Err(err)) = rosetta_resp.metadata else {
        panic!("Expected submit to error with dry-run: INSUFFICIENT_GAS");
    };

    assert_eq!(
        err,
        RosettaError {
            code: 11,
            message: "Transaction dry run error".to_string(),
            description: None,
            retriable: false,
            details: Some(
                json!({"error": "ExecutionError: Kind: INSUFFICIENT_GAS, Description: InsufficientGas"})
            )
        }
    );
    Ok(())
}
