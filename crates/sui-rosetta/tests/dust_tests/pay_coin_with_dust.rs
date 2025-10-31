// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;
use std::path::Path;

use crate::test_utils::wait_for_transaction;
use prost_types::FieldMask;
use serde_json::json;
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::operations::Operations;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use test_cluster::TestClusterBuilder;

use super::rosetta_client::{RosettaError, start_rosetta_test_server};

#[allow(dead_code)]
#[path = "../custom_coins/test_coin_utils.rs"]
mod test_coin_utils;
use test_coin_utils::{TEST_COIN_DECIMALS, init_package, mint};

#[tokio::test]
async fn test_pay_custom_coin_with_multiple_coins() -> anyhow::Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

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

    let coin_balance = 1_000_000_u64;
    let n_coins = 10_usize;
    let total_balance = n_coins as i128 * coin_balance as i128;
    let balances_to = vec![(coin_balance, sender); n_coins];
    // Create 10 coins to transfer later at once
    let _mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // Test rosetta can handle using many "small" coins for payment
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops: Operations = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PayCoin",
            "account": { "address" : recipient.to_string() },
            "amount" : {
                "value": total_balance.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        },
        {
            "operation_identifier":{"index":1},
            "type":"PayCoin",
            "account": { "address" : sender.to_string() },
            "amount" : {
                "value": (-total_balance).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        }]
    ))
    .unwrap();

    let submit = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(&mut client, &submit.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(submit.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
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

    // Create coin cache for testing Operations conversion
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let ops2 = Operations::try_from_executed_transaction(tx, &coin_cache)
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
async fn test_pay_custom_coin_no_balance() -> anyhow::Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

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

    let total_balance = 1_000_000_i128;
    // Test rosetta can handle using many "small" coins for payment
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops: Operations = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PayCoin",
            "account": { "address" : recipient.to_string() },
            "amount" : {
                "value": total_balance.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        },
        {
            "operation_identifier":{"index":1},
            "type":"PayCoin",
            "account": { "address" : sender.to_string() },
            "amount" : {
                "value": (-total_balance).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        }]
    ))
    .unwrap();

    let resps = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    let Some(Err(e)) = resps.metadata else {
        panic!("Expected metadata to exist and error")
    };
    let details = Some(json!(
        {
            "error": format!("status: 'The system is not in a state required for the operation's execution', self: \"Insufficient funds for address [{sender}], requested amount: {total_balance}, total available: 0\""),
        }
    ));
    assert_eq!(
        e,
        RosettaError {
            code: 16,
            message: "Sui rpc error".to_string(),
            description: None,
            retriable: false,
            details,
        },
    );

    Ok(())
}

#[tokio::test]
async fn test_pay_custom_coin_with_multiple_merge_chunks() -> anyhow::Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut init_ret = init_package(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let coin_balance = 1_000_000_u64;
    let n_coins = 500_usize;
    let total_balance = 2 * n_coins as i128 * coin_balance as i128;
    let balances_to = vec![(coin_balance, sender); n_coins];
    // Create 10 coins to transfer later at once
    let mint_res0 = mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret.clone(),
        balances_to.clone(),
    )
    .await
    .unwrap();
    let t_cap_obj = mint_res0
        .effects()
        .changed_objects
        .iter()
        .find(|obj| {
            obj.object_type_opt()
                .map(|t| t.contains("TreasuryCap"))
                .unwrap_or(false)
        })
        .expect("Expected changed_objects to contain TreasuryCap");

    let t_cap_ref = (
        t_cap_obj.object_id().parse().unwrap(),       // new object_id
        t_cap_obj.output_version.unwrap_or(0).into(), // new version
        t_cap_obj.output_digest().parse().unwrap(),   // new digest
    );
    init_ret.treasury_cap = t_cap_ref;

    // Update changed_objects list to include all objects from first mint
    let mut new_changed_objects = Vec::new();
    for obj in &mint_res0.effects().changed_objects {
        if let Some(object_id_str) = &obj.object_id
            && let Ok(object_id) = object_id_str.parse::<sui_types::base_types::ObjectID>()
        {
            new_changed_objects.push(object_id);
        }
    }
    init_ret.changed_objects.extend(new_changed_objects);

    let _mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // Test rosetta can handle using many "small" coins for payment
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops: Operations = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PayCoin",
            "account": { "address" : recipient.to_string() },
            "amount" : {
                "value": total_balance.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        },
        {
            "operation_identifier":{"index":1},
            "type":"PayCoin",
            "account": { "address" : sender.to_string() },
            "amount" : {
                "value": (-total_balance).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        }]
    ))
    .unwrap();

    let submit = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(&mut client, &submit.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(submit.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
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

    // Create coin cache for testing Operations conversion
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let ops2 = Operations::try_from_executed_transaction(tx, &coin_cache)
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
