// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod rosetta_client;
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

use serde_json::json;
use std::path::Path;
use sui_rosetta::grpc_client::GrpcClient;
use sui_rosetta::types::Currencies;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Currency, CurrencyMetadata,
    NetworkIdentifier, SuiEnv,
};
use sui_rosetta::SUI;
use test_cluster::TestClusterBuilder;
use test_coin_utils::{init_package, mint};
use url::Url;

use crate::rosetta_client::{start_rosetta_test_server_with_rpc_url, RosettaEndpoint};

#[tokio::test]
async fn test_custom_coin_balance() {
    // mint coins to `test_culset.get_address_1()` and `test_culset.get_address_2()`
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // Create GRPC client for test utilities
    let grpc_client =
        GrpcClient::new(Url::parse(test_cluster.rpc_url()).unwrap(), None, None).unwrap();

    let (rosetta_client, _handle) =
        start_rosetta_test_server_with_rpc_url(test_cluster.rpc_url()).await;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(
        &client,
        &grpc_client,
        keystore,
        sender,
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/custom_coins/test_coin"
        )),
    )
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let _mint_res = mint(&client, &grpc_client, keystore, init_ret, balances_to)
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

    let (rosetta_client, _handles) =
        start_rosetta_test_server_with_rpc_url(test_cluster.rpc_url()).await;

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

    // Create GRPC client for test utilities
    let grpc_client =
        GrpcClient::new(Url::parse(test_cluster.rpc_url()).unwrap(), None, None).unwrap();

    // TEST_COIN setup and mint
    let init_ret = init_package(
        &client,
        &grpc_client,
        keystore,
        sender,
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/custom_coins/test_coin"
        )),
    )
    .await
    .unwrap();
    let balances_to = vec![(COIN1_BALANCE, sender)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);
    let _mint_res = mint(&client, &grpc_client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    let (rosetta_client, _handle) =
        start_rosetta_test_server_with_rpc_url(test_cluster.rpc_url()).await;

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

    // Wait a bit for the transaction to be indexed
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Get transaction using GRPC
    let grpc_client2 =
        GrpcClient::new(Url::parse(test_cluster.rpc_url()).unwrap(), None, None).unwrap();
    let tx_with_details = grpc_client2
        .get_transaction_with_details(response.transaction_identifier.hash)
        .await
        .unwrap();

    // Check that transaction succeeded
    let effects = tx_with_details.effects.as_ref().expect("Missing effects");
    let status = effects.status.as_ref().expect("Missing status");
    assert!(status.success.unwrap_or(false), "Transaction failed");

    println!("Sui TX: {:?}", tx_with_details);

    // Verify the balance changes match what we expected
    let balance_changes = &tx_with_details.balance_changes;
    assert!(!balance_changes.is_empty(), "No balance changes found");

    // Find the transfer balance changes
    let mut found_sender_decrease = false;
    let mut found_recipient_increase = false;

    for change in balance_changes {
        if let (Some(address), Some(coin_type), Some(amount)) =
            (&change.address, &change.coin_type, &change.amount)
        {
            if coin_type.contains("test_coin::TEST_COIN") {
                if address == &sender.to_string() && amount == "-30000000" {
                    found_sender_decrease = true;
                } else if address == &recipient.to_string() && amount == "30000000" {
                    found_recipient_increase = true;
                }
            }
        }
    }

    assert!(
        found_sender_decrease,
        "Did not find sender balance decrease"
    );
    assert!(
        found_recipient_increase,
        "Did not find recipient balance increase"
    );
}

#[tokio::test]
async fn test_custom_coin_without_symbol() {
    const COIN1_BALANCE: u64 = 100_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // Create GRPC client for test utilities
    let grpc_client =
        GrpcClient::new(Url::parse(test_cluster.rpc_url()).unwrap(), None, None).unwrap();

    // TEST_COIN setup and mint
    let init_ret = init_package(
        &client,
        &grpc_client,
        keystore,
        sender,
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/custom_coins/test_coin_no_symbol"
        )),
    )
    .await
    .unwrap();

    let balances_to = vec![(COIN1_BALANCE, sender)];
    let mint_res = mint(&client, &grpc_client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // Get transaction using GRPC instead of read_api
    let grpc_client2 =
        GrpcClient::new(Url::parse(test_cluster.rpc_url()).unwrap(), None, None).unwrap();
    let tx_with_details = grpc_client2
        .get_transaction_with_details(mint_res.digest)
        .await
        .unwrap();

    // Check that transaction succeeded
    let effects = tx_with_details.effects.as_ref().expect("Missing effects");
    let status = effects.status.as_ref().expect("Missing status");
    assert!(status.success.unwrap_or(false), "Transaction failed");

    // Check balance changes directly from the proto transaction
    let balance_changes = &tx_with_details.balance_changes;
    assert!(
        !balance_changes.is_empty(),
        "No balance changes found in transaction"
    );

    let has_non_empty_symbols = balance_changes.iter().any(|change| {
        // Any balance change should have a coin type
        change.coin_type.is_some()
    });
    assert!(
        has_non_empty_symbols,
        "Expected to find balance changes with coin types"
    );
}
