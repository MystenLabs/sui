use std::path::Path;

use serde_json::json;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_rosetta::operations::Operations;
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

    println!("Sui TX: {tx:?}");

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

// TEST: Multiple chunks of merge
//
// TEST: Gas-coins-need ordering

// TEST: Multiple chunks of merge
//
// TEST: Gas-coins-need ordering
