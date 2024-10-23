// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod rosetta_client;
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

use serde_json::json;
use std::num::NonZeroUsize;
use std::path::Path;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Currency, CurrencyMetadata,
    NetworkIdentifier, SuiEnv,
};
use sui_rosetta::types::{Currencies, OperationType};
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::SUI;
use test_cluster::TestClusterBuilder;
use test_coin_utils::{init_package, mint};

use crate::rosetta_client::{start_rosetta_test_server, RosettaEndpoint};

#[tokio::test]
async fn test_custom_coin_balance() {
    // mint coins to `test_culset.get_address_1()` and `test_culset.get_address_2()`
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let _mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // setup AccountBalanceRequest
    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };

    let sui_currency = SUI.clone();
    let test_coin_currency = Currency {
        symbol: "TEST_COIN".to_string(),
        decimals: 6,
        metadata: CurrencyMetadata {
            coin_type: coin_type.clone(),
        },
    };

    // Verify initial balance and stake
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address: address1,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: Currencies(vec![sui_currency, test_coin_currency]),
    };

    println!(
        "request: {}",
        serde_json::to_string_pretty(&request).unwrap()
    );
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    println!(
        "response: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );
    assert_eq!(response.balances.len(), 2);
    assert_eq!(response.balances[0].value, SUI_BALANCE as i128);
    assert_eq!(
        response.balances[0].currency.clone().metadata.coin_type,
        "0x2::sui::SUI"
    );
    assert_eq!(response.balances[1].value, COIN1_BALANCE as i128);
    assert_eq!(
        response.balances[1].currency.clone().metadata.coin_type,
        coin_type
    );
}

#[tokio::test]
async fn test_default_balance() {
    // mint coins to `test_culset.get_address_1()` and `test_culset.get_address_2()`
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();

    let (rosetta_client, _handles) = start_rosetta_test_server(client.clone()).await;

    let request: AccountBalanceRequest = serde_json::from_value(json!(
        {
            "network_identifier": {
                "blockchain": "sui",
                "network": "localnet"
            },
            "account_identifier": {
                "address": test_cluster.get_address_0()
            }
        }
    ))
    .unwrap();
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    println!(
        "response: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );
    assert_eq!(response.balances.len(), 1);
    assert_eq!(response.balances[0].value, SUI_BALANCE as i128);

    // Keep server running for testing with bash/curl
    // To test with curl,
    // 1. Uncomment the following lines
    // 2. use `cargo test -- --nocapture` to print the server <port> and <address0>
    // 3. run curl 'localhost:<port>/account/balance' --header 'Content-Type: application/json' \
    // --data-raw '{
    //     "network_identifier": {
    //         "blockchain": "sui",
    //         "network": "localnet"
    //     },
    //     "account_identifier": {
    //         "address": "<address0 above>"
    //     }
    // }'
    // println!("port: {}", rosetta_client.online_port());
    // println!("address0: {}", test_cluster.get_address_0());
    // for handle in _handles.into_iter() {
    //     handle.await.unwrap();
    // }
}

#[tokio::test]
async fn test_custom_coin_transfer() {
    const COIN1_BALANCE: u64 = 100_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // TEST_COIN setup and mint
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let balances_to = vec![(COIN1_BALANCE, sender)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);
    let _mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PayCoin",
            "account": { "address" : recipient.to_string() },
            "amount" : {
                "value": "30000000",
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": 6,
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
                "value": "-30000000",
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": 6,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        }]
    ))
    .unwrap();

    let response = rosetta_client.rosetta_flow(&ops, keystore).await;

    let tx = client
        .read_api()
        .get_transaction_with_options(
            response.transaction_identifier.hash,
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
    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let ops2 = Operations::try_from_response(tx, &coin_cache)
        .await
        .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );
}

#[tokio::test]
async fn test_custom_coin_without_symbol() {
    const COIN1_BALANCE: u64 = 100_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // TEST_COIN setup and mint
    let init_ret = init_package(
        &client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin_no_symbol"),
    )
    .await
    .unwrap();

    let balances_to = vec![(COIN1_BALANCE, sender)];
    let mint_res = mint(&client, keystore, init_ret, balances_to)
        .await
        .unwrap();

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
    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let ops = Operations::try_from_response(tx, &coin_cache)
        .await
        .unwrap();

    for op in ops {
        if op.type_ == OperationType::SuiBalanceChange {
            assert!(!op.amount.unwrap().currency.symbol.is_empty())
        }
    }
}
