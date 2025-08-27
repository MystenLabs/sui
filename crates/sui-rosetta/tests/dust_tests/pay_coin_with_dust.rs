// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;
use std::path::Path;

use serde_json::json;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_rosetta::operations::Operations;
use sui_rosetta::CoinMetadataCache;
use test_cluster::TestClusterBuilder;

use super::rosetta_client::{start_rosetta_test_server, RosettaError};

#[allow(dead_code)]
#[path = "../custom_coins/test_coin_utils.rs"]
mod test_coin_utils;
use test_coin_utils::{init_package, mint, TEST_COIN_DECIMALS};

#[tokio::test]
async fn test_pay_custom_coin_with_multiple_coins() -> anyhow::Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let init_ret = init_package(
        &client,
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
    let _mint_res = mint(&client, keystore, init_ret, balances_to)
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

    let tx = client
        .read_api()
        .get_transaction_with_options(
            submit.transaction_identifier.hash,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects()
                .with_balance_changes()
                .with_events(),
        )
        .await
        .unwrap();

    assert_eq!(
        &SuiExecutionStatus::Success,
        tx.effects.as_ref().unwrap().status()
    );

    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let ops2 = Operations::try_from_response(tx, &coin_cache)
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
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let init_ret = init_package(
        &client,
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
            "error": format!("Insufficient fund for address [{sender}], requested amount: {total_balance}"),
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
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut init_ret = init_package(
        &client,
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
    let mint_res0 = mint(&client, keystore, init_ret.clone(), balances_to.clone())
        .await
        .unwrap();
    let t_cap_ref = mint_res0
        .object_changes
        .as_ref()
        .expect("Expected object_changes")
        .iter()
        .find(|&chng| chng.object_id() == init_ret.treasury_cap.0)
        .expect("Expected object_changes to contain coin_object_id")
        .object_ref();
    init_ret.treasury_cap = t_cap_ref;

    let mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // Make sure the tx has registered
    let tx = client
        .read_api()
        .get_transaction_with_options(
            mint_res.digest,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects()
                .with_balance_changes()
                .with_events(),
        )
        .await
        .unwrap();

    assert_eq!(
        &SuiExecutionStatus::Success,
        tx.effects.as_ref().unwrap().status()
    );

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

    let tx = client
        .read_api()
        .get_transaction_with_options(
            submit.transaction_identifier.hash,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects()
                .with_balance_changes()
                .with_events(),
        )
        .await
        .unwrap();

    assert_eq!(
        &SuiExecutionStatus::Success,
        tx.effects.as_ref().unwrap().status()
    );

    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let ops2 = Operations::try_from_response(tx, &coin_cache)
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
