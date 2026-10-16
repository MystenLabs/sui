// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(dead_code)]
mod rosetta_client;
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils;

use std::num::NonZeroUsize;

use prost_types::FieldMask;
use serde_json::json;
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;

use sui_rosetta::operations::Operations;
mod test_utils;
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::SUI;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Amount, Currency,
    CurrencyMetadata, NetworkIdentifier, SuiEnv,
};
use sui_rosetta::types::{Currencies, OperationType};
use test_cluster::TestClusterBuilder;
use test_coin_utils::{TEST_COIN_DECIMALS, init_package, mint};
use test_utils::{
    get_sui_balance, get_total_balance, send_all_sui_coins, send_sui_coin, wait_for_transaction,
};

use crate::rosetta_client::{RosettaEndpoint, start_rosetta_test_server};

#[tokio::test]
async fn test_mint() {
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(&test_cluster, &mut client, keystore, sender, &{
        let mut test_coin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_coin_path.push("tests/custom_coins/test_coin");
        test_coin_path
    })
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];

    let mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
        .await
        .unwrap();
    let coins = mint_res
        .objects_opt()
        .map(|object_set| {
            object_set
                .objects
                .iter()
                .filter(|obj| {
                    if let Some(object_type) = &obj.object_type {
                        object_type.contains("Coin<")
                    } else {
                        false
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let coin1 = coins
        .iter()
        .find(|obj| {
            obj.owner_opt()
                .and_then(|owner| owner.address_opt())
                .is_some_and(|addr| addr == address1.to_string())
        })
        .unwrap();
    let coin2 = coins
        .iter()
        .find(|obj| {
            obj.owner_opt()
                .and_then(|owner| owner.address_opt())
                .is_some_and(|addr| addr == address2.to_string())
        })
        .unwrap();
    assert!(coin1.object_type().contains("::test_coin::TEST_COIN"));
    assert!(coin2.object_type().contains("::test_coin::TEST_COIN"));
}

#[tokio::test]
async fn test_custom_coin_balance() {
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(&test_cluster, &mut client, keystore, sender, &{
        let mut test_coin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_coin_path.push("tests/custom_coins/test_coin");
        test_coin_path
    })
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let _mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };

    let sui_currency = SUI.clone();
    let test_coin_currency = Currency {
        symbol: "TEST_COIN".to_string(),
        decimals: TEST_COIN_DECIMALS,
        metadata: CurrencyMetadata {
            coin_type: coin_type.clone(),
        },
    };

    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address: address1,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: Currencies(vec![sui_currency, test_coin_currency]),
    };

    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await
        .unwrap();
    assert_eq!(response.balances.len(), 2);
    assert_eq!(response.balances[0].value, SUI_BALANCE as i128);
    assert_eq!(
        response.balances[0].currency.clone().metadata.coin_type,
        "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI"
    );
    assert_eq!(response.balances[1].value, COIN1_BALANCE as i128);
    assert_eq!(
        response.balances[1].currency.clone().metadata.coin_type,
        coin_type
    );
}

#[tokio::test]
async fn test_default_balance() {
    const SUI_BALANCE: u64 = 150_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handles) = start_rosetta_test_server(client).await;

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
        .await
        .unwrap();
    assert_eq!(response.balances.len(), 1);
    assert_eq!(response.balances[0].value, SUI_BALANCE as i128);
}

#[tokio::test]
async fn test_custom_coin_transfer() {
    const COIN1_BALANCE: u64 = 100_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let init_ret = init_package(&test_cluster, &mut client, keystore, sender, &{
        let mut test_coin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_coin_path.push("tests/custom_coins/test_coin");
        test_coin_path
    })
    .await
    .unwrap();
    let balances_to = vec![(COIN1_BALANCE, sender)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);
    let _mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
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
                "value": "-30000000",
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

    let flow_response = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let response = flow_response.submit.unwrap().unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
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
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let tx_digest = tx.digest.expect("Expected transaction digest");

    let grpc_request = GetTransactionRequest::default()
        .with_digest(tx_digest.clone())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events.events.event_type",
            "events.events.json",
            "events.events.contents",
        ]));

    let mut client_copy = client.clone();
    let grpc_response = client_copy
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
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );
}

#[tokio::test]
async fn test_custom_coin_without_symbol() {
    const COIN1_BALANCE: u64 = 100_000_000_000_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let init_ret = init_package(&test_cluster, &mut client, keystore, sender, &{
        let mut test_coin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_coin_path.push("tests/custom_coins/test_coin_no_symbol");
        test_coin_path
    })
    .await
    .unwrap();

    let balances_to = vec![(COIN1_BALANCE, sender)];
    let mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(mint_res.digest().to_string())
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
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let tx_digest = tx.digest.expect("Expected transaction digest");

    let grpc_request = GetTransactionRequest::default()
        .with_digest(tx_digest.clone())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events.events.event_type",
            "events.events.json",
            "events.events.contents",
        ]));

    let mut client_copy = client.clone();
    let grpc_response = client_copy
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let executed_tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");
    let ops = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();

    for op in ops {
        if op.type_ == OperationType::SuiBalanceChange {
            assert!(!op.amount.unwrap().currency.symbol.is_empty())
        }
    }
}

#[tokio::test]
async fn test_mint_with_gas_coin_transfer() -> anyhow::Result<()> {
    const COIN1_BALANCE: u64 = 100_000_000;
    const COIN2_BALANCE: u64 = 200_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let sender = test_cluster.get_address_0();
    let init_ret = init_package(&test_cluster, &mut client, keystore, sender, &{
        let mut test_coin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_coin_path.push("tests/custom_coins/test_coin");
        test_coin_path
    })
    .await
    .unwrap();

    let address1 = test_cluster.get_address_1();
    let address2 = test_cluster.get_address_2();
    let balances_to = vec![(COIN1_BALANCE, address1), (COIN2_BALANCE, address2)];

    let mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
        .await
        .unwrap();
    let effects = mint_res.effects();
    let gas_summary = effects.gas_used();
    let gas_used_amount = gas_summary.computation_cost.unwrap_or(0)
        + gas_summary.storage_cost.unwrap_or(0)
        - gas_summary.storage_rebate.unwrap_or(0);
    let mut gas_used = gas_used_amount as i128;

    let coins = mint_res
        .objects_opt()
        .map(|object_set| {
            object_set
                .objects
                .iter()
                .filter(|obj| {
                    if let Some(object_type) = &obj.object_type {
                        object_type.contains("Coin<")
                    } else {
                        false
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let coin1 = coins
        .iter()
        .find(|obj| {
            obj.owner_opt()
                .and_then(|owner| owner.address_opt())
                .is_some_and(|addr| addr == address1.to_string())
        })
        .unwrap();
    let coin2 = coins
        .iter()
        .find(|obj| {
            obj.owner_opt()
                .and_then(|owner| owner.address_opt())
                .is_some_and(|addr| addr == address2.to_string())
        })
        .unwrap();
    assert!(coin1.object_type().contains("::test_coin::TEST_COIN"));
    assert!(coin2.object_type().contains("::test_coin::TEST_COIN"));

    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let executed_tx = mint_res;

    let ops = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();
    const COIN_BALANCE_CREATED: u64 = COIN1_BALANCE + COIN2_BALANCE;
    let mut coin_created = 0;
    ops.into_iter().for_each(|op| {
        if let Some(Amount {
            value, currency, ..
        }) = op.amount
        {
            if currency == Currency::default() {
                gas_used += value
            } else {
                coin_created += value
            }
        }
    });

    assert!(COIN_BALANCE_CREATED as i128 == coin_created);
    assert!(gas_used == 0);

    Ok(())
}

#[tokio::test]
async fn test_pay_coin_estimate_only() {
    const COIN_BALANCE: u64 = 100_000_000;
    const PAY_AMOUNT: u64 = 30_000_000;
    let test_cluster = TestClusterBuilder::new().build().await;
    let minter = test_cluster.get_address_0();
    let sender = test_cluster.get_address_1();
    let recipient = test_cluster.get_address_2();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let init_ret = init_package(&test_cluster, &mut client, keystore, minter, &{
        let mut test_coin_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        test_coin_path.push("tests/custom_coins/test_coin");
        test_coin_path
    })
    .await
    .unwrap();
    let balances_to = vec![(COIN_BALANCE, sender)];
    let coin_type = init_ret.coin_tag.to_canonical_string(true);
    let _mint_res = mint(&test_cluster, &mut client, keystore, init_ret, balances_to)
        .await
        .unwrap();

    // Leave the sender holding only the test coin: no SUI coins and no SUI address balance.
    send_all_sui_coins(&mut client, keystore, sender, minter)
        .await
        .unwrap();
    assert_eq!(0, get_sui_balance(&mut client, sender).await);

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops: Operations = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PayCoin",
            "account": { "address" : recipient.to_string() },
            "amount" : {
                "value": PAY_AMOUNT.to_string(),
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
                "value": format!("-{PAY_AMOUNT}"),
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

    // An estimate-only metadata request quotes the gas budget even though the sender has no SUI.
    let estimate = rosetta_client
        .estimate_gas(&ops)
        .await
        .expect("estimate_only metadata should succeed for a sender without SUI");
    assert!(estimate.metadata.estimate_only);
    assert!(estimate.metadata.gas_coins.is_empty());
    assert_eq!(
        Some(coin_type.as_str()),
        estimate
            .metadata
            .currency
            .as_ref()
            .map(|currency| currency.metadata.coin_type.as_str())
    );
    let quoted_fee = estimate.suggested_fee[0].value;
    assert!(quoted_fee > 0);
    assert_eq!(quoted_fee, estimate.metadata.budget as i128);

    // Without estimate_only, metadata fails gas selection.
    let error = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .metadata
        .expect("metadata step should run")
        .expect_err("metadata should fail for a sender without SUI");
    assert_eq!(16, error.code, "unexpected error: {error:?}");
    let details = error.details.as_ref().expect("error should carry details");
    let message = details["error"].as_str().expect("details.error string");
    assert!(
        message.contains("Unable to perform gas selection"),
        "unexpected error message: {message}"
    );
    // The error carries the same budget as the quote, plus the balances gas selection saw.
    assert_eq!(
        Some(estimate.metadata.budget.to_string().as_str()),
        details["required_budget"].as_str()
    );
    assert_eq!(Some("0"), details["coin_balance"].as_str());
    assert_eq!(Some("0"), details["address_balance"].as_str());

    // With no other SUI, a single coin worth the quoted fee pays exactly the quoted budget.
    send_sui_coin(
        &mut client,
        keystore,
        minter,
        sender,
        estimate.metadata.budget,
    )
    .await
    .unwrap();

    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    assert_eq!(
        estimate.metadata.budget,
        flow.metadata.unwrap().unwrap().metadata.budget
    );
    let response = flow.submit.unwrap().unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events.events.event_type",
            "events.events.json",
            "events.events.contents",
        ]));

    let executed_tx = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner()
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        executed_tx.effects().status().success(),
        "Transaction failed: {:?}",
        executed_tx.effects().status().error()
    );

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let ops2 = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );

    assert_eq!(
        PAY_AMOUNT,
        get_total_balance(&mut client, recipient, &coin_type).await
    );
}
