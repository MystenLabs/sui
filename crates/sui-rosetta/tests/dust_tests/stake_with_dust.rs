// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use anyhow::{Result, anyhow};
use prost_types::FieldMask;
use serde_json::json;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::{CoinMetadataCache, operations::Operations};
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetObjectsRequest, GetEpochRequest, GetObjectRequest, GetTransactionRequest,
};
use sui_types::{
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
};
use test_cluster::TestClusterBuilder;

use crate::test_utils::{execute_transaction, get_all_coins, get_object_ref, wait_for_transaction};
use crate::{
    rosetta_client::start_rosetta_test_server,
    split_coin::{DEFAULT_GAS_BUDGET, make_change},
};

#[tokio::test]
async fn test_stake_with_many_small_coins() -> Result<()> {
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
    let mut all_coins_iter = all_coins_sender.into_iter();
    let coin_to_split = all_coins_iter.next().unwrap();
    let _gas_for_split_tx = all_coins_iter.next().unwrap();

    let coin_to_split_id = coin_to_split.id();
    let gas_for_split_tx_id = _gas_for_split_tx.id();

    let gas_price = client.get_reference_gas_price().await?;

    // With 150K SUI, stake 50K SUI split into 300 small coins
    let amount_to_stake: i128 = 50_000_000_000_000; // 50K SUI
    let num_coins = 300;
    let split_amount = (amount_to_stake / num_coins) as u64; // ~0.167 SUI per coin
    assert!(
        num_coins > 255,
        "Test requires more than 255 coins to test multiple merge commands"
    );

    // Send rest of the coins to recipient first
    for coin in all_coins_iter {
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
        let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        // Check that the transaction was successful
        let effects = resp.effects();
        assert!(
            effects.status().success(),
            "Something went wrong sending coins"
        );
    }
    // Fetch the updated state of the first two coins directly from the ledger
    // This avoids relying on the potentially stale ownership index
    use prost_types::FieldMask;
    let mut first_coin_request = GetObjectRequest::default();
    first_coin_request.object_id = Some(coin_to_split_id.to_string());

    let mut second_coin_request = GetObjectRequest::default();
    second_coin_request.object_id = Some(gas_for_split_tx_id.to_string());

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

    let coin_to_split_obj = objects[0].object();
    let gas_for_split_tx_obj = objects[1].object();

    // Convert the proto objects to Object
    let coin_to_split = coin_to_split_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;
    let gas_for_split_tx = gas_for_split_tx_obj
        .bcs()
        .deserialize::<sui_types::object::Object>()?;

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
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

    // Check that the transaction was successful
    let effects = resp.effects();
    assert!(
        effects.status().success(),
        "Something went wrong sending coins"
    );

    // Test rosetta can handle using many "small" coins for payment
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));

    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();

    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();

    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<sui_types::base_types::SuiAddress>()
        .unwrap();

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": (-amount_to_stake).to_string() },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
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
async fn test_stake_with_multiple_merges() -> Result<()> {
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
    let mut all_coins_iter = all_coins_sender.into_iter();
    let coin_to_split = all_coins_iter.next().unwrap();
    let _gas_for_split_tx = all_coins_iter.next().unwrap();

    let gas_price = client.get_reference_gas_price().await?;

    // With 150K SUI, stake 50K SUI split into 866 small coins to test multiple merges
    let amount_to_stake: i128 = 50_000_000_000_000; // 50K SUI
    let num_coins = 255 + 511 + 100; // 866 coins - tests multiple merge operations
    let split_amount = (amount_to_stake / num_coins) as u64; // ~0.058 SUI per coin
    assert!(
        num_coins > 255 + 511,
        "Test requires more than 255 + 511 coins to test multiple merge operations"
    );

    // Send rest of the coins to recipient first
    for coin in all_coins_iter {
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
        let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        // Check that the transaction was successful
        let effects = resp.effects();
        assert!(
            effects.status().success(),
            "Something went wrong sending coins"
        );
    }
    // Update references of existing coin objects
    let mut all_coins = get_all_coins(&mut client.clone(), sender).await?;
    let (coin_to_split, gas_for_split_tx) = match all_coins.as_mut_slice() {
        [coin_0, _] => {
            if *coin_0.id() == *coin_to_split.id() {
                let gas_for_split_tx = all_coins.pop().unwrap();
                (all_coins.pop().unwrap(), gas_for_split_tx)
            } else {
                (all_coins.pop().unwrap(), all_coins.pop().unwrap())
            }
        }
        _ => {
            unreachable!("should have sent away other coins now");
        }
    };

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
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

    // Check that the transaction was successful
    let effects = resp.effects();
    assert!(
        effects.status().success(),
        "Something went wrong sending coins"
    );

    // Test rosetta can handle using many "small" coins for payment
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, mut _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));

    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();

    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();

    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<sui_types::base_types::SuiAddress>()
        .unwrap();

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": (-amount_to_stake).to_string() },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
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

    Ok(())
}

#[tokio::test]
async fn test_stake_with_coin_limit() -> Result<()> {
    use crate::test_utils::get_coin_value;
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
    // Save the first two coins for later use
    let first_coin_id = all_coins_sender[0].id();
    let second_coin_id = all_coins_sender[1].id();

    let gas_price = client.get_reference_gas_price().await?;

    let mut gas_for_transfers = all_coins_sender[0].compute_object_reference();

    // Send rest of the coins to recipient first
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
        let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

        // Check that the transaction was successful
        let effects = resp.effects();
        if !effects.status().success() {
            let error = effects.status().error_opt();
            panic!("Something went wrong sending coins: {:?}", error);
        }

        gas_for_transfers = get_object_ref(&mut client.clone(), all_coins_sender[0].id())
            .await?
            .as_object_ref();
    }

    // Fetch the updated state of the first two coins directly from the ledger
    // This avoids relying on the potentially stale ownership index
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

    // Convert the proto objects to Object
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
    let split_amount = get_coin_value(&coin_to_split) / (target_coins + 1); // ~75M MIST per coin

    // Split balance to create many coins
    let _ = make_change(
        &mut client.clone(),
        keystore,
        sender,
        &coin_to_split,
        Some(gas_for_split_tx.compute_object_reference()),
        split_amount,
    )
    .await?;

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
        DEFAULT_GAS_BUDGET,
        gas_price,
    );
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let resp = execute_transaction(&mut client.clone(), &signed_transaction).await?;

    // Check that the transaction was successful
    let effects = resp.effects();
    assert!(
        effects.status().success(),
        "Something went wrong sending coins"
    );

    // Test rosetta can handle staking with many "small" coins
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Request staking amount that uses most of the 1500 coins
    // We have ~2000 coins total, rosetta will select up to 1500
    // Stake amount for ~1200 coins to leave more buffer for gas (staking requires more gas than payment)
    let amount_to_stake = (split_amount as i128 * 1200).to_string();

    // Find an available validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<sui_types::base_types::SuiAddress>()
        .unwrap();

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": format!("-{}", amount_to_stake) },
            "metadata": {
                "Stake": {
                    "sender": sender.to_string(),
                    "validator": validator,
                    "amount": amount_to_stake
                }
            }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    // Wait for the transaction to be available in the ledger
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

    // Verify the transaction succeeded
    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );

    Ok(())
}
