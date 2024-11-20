// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use serde_json::json;
use shared_crypto::intent::Intent;
use std::num::NonZeroUsize;
use std::time::Duration;
use sui_types::transaction::Transaction;

use rosetta_client::start_rosetta_test_server;
use split_coin::{make_change, DEFAULT_GAS_BUDGET};
use sui_json_rpc_types::SuiTransactionBlockResponseOptions;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::Currencies;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Currency, NetworkIdentifier,
    SubAccount, SubAccountType, SuiEnv,
};
use sui_rosetta::CoinMetadataCache;
use sui_sdk::rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_swarm_config::genesis_config::{DEFAULT_GAS_AMOUNT, DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT};
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::utils::to_sender_signed_transaction;
use test_cluster::TestClusterBuilder;

use crate::rosetta_client::RosettaEndpoint;

mod rosetta_client;
mod split_coin;

#[tokio::test]
async fn test_get_staked_sui() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    tokio::time::sleep(Duration::from_secs(1)).await;

    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };
    // Verify initial balance and stake
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: Currencies(vec![Currency::default()]),
    };

    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    assert_eq!(1, response.balances.len());
    assert_eq!(
        (DEFAULT_GAS_AMOUNT * DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT as u64) as i128,
        response.balances[0].value
    );

    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: Some(SubAccount {
                account_type: SubAccountType::PendingStake,
            }),
        },
        block_identifier: Default::default(),
        currencies: Currencies(vec![Currency::default()]),
    };
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await;
    assert_eq!(response.balances[0].value, 0);

    // Stake some sui
    let validator = client
        .governance_api()
        .get_latest_sui_system_state()
        .await
        .unwrap()
        .active_validators[0]
        .sui_address;
    let coins = client
        .coin_read_api()
        .get_coins(address, None, None, None)
        .await
        .unwrap()
        .data;
    let delegation_tx = client
        .transaction_builder()
        .request_add_stake(
            address,
            vec![coins[0].coin_object_id],
            Some(1_000_000_000),
            validator,
            None,
            1_000_000_000,
        )
        .await
        .unwrap();
    let tx = to_sender_signed_transaction(delegation_tx, keystore.export(&address).unwrap());
    client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::new(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();

    let response = rosetta_client
        .get_balance(
            network_identifier.clone(),
            address,
            Some(SubAccountType::PendingStake),
        )
        .await;
    assert_eq!(1, response.balances.len());
    assert_eq!(1_000_000_000, response.balances[0].value);

    println!("{}", serde_json::to_string_pretty(&response).unwrap());
}

#[tokio::test]
async fn test_stake() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = client
        .governance_api()
        .get_latest_sui_system_state()
        .await
        .unwrap()
        .active_validators[0]
        .sui_address;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
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

    println!("Sui TX: {tx:?}");

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
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );

    println!("{}", serde_json::to_string_pretty(&ops2).unwrap())
}

#[tokio::test]
async fn test_stake_all() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = client
        .governance_api()
        .get_latest_sui_system_state()
        .await
        .unwrap()
        .active_validators[0]
        .sui_address;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
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

    println!("Sui TX: {tx:?}");

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
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );

    println!("{}", serde_json::to_string_pretty(&ops2).unwrap())
}

#[tokio::test]
async fn test_withdraw_stake() {
    telemetry_subscribers::init_for_testing();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(60000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // First add some stakes
    let validator = client
        .governance_api()
        .get_latest_sui_system_state()
        .await
        .unwrap()
        .active_validators[0]
        .sui_address;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
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

    println!("Sui TX: {tx:?}");

    assert_eq!(
        &SuiExecutionStatus::Success,
        tx.effects.as_ref().unwrap().status()
    );
    // verify balance
    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };
    let response = rosetta_client
        .get_balance(
            network_identifier.clone(),
            sender,
            Some(SubAccountType::PendingStake),
        )
        .await;

    assert_eq!(1, response.balances.len());
    assert_eq!(1000000000, response.balances[0].value);

    // Trigger epoch change.
    test_cluster.trigger_reconfiguration().await;

    // withdraw all stake
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"WithdrawStake",
            "account": { "address" : sender.to_string() }
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

    println!("{}", serde_json::to_string_pretty(&ops2).unwrap());

    // stake should be 0
    let response = rosetta_client
        .get_balance(
            network_identifier.clone(),
            sender,
            Some(SubAccountType::PendingStake),
        )
        .await;

    assert_eq!(1, response.balances.len());
    assert_eq!(0, response.balances[0].value);
}

#[tokio::test]
async fn test_pay_sui() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": "1000000000" }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" }
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
async fn test_pay_sui_multiple_times() {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    for i in 1..20 {
        println!("Iteration: {}", i);
        let ops = serde_json::from_value(json!(
            [{
                "operation_identifier":{"index":0},
                "type":"PaySui",
                "account": { "address" : recipient.to_string() },
                "amount" : { "value": "1000000000" }
            },{
                "operation_identifier":{"index":1},
                "type":"PaySui",
                "account": { "address" : sender.to_string() },
                "amount" : { "value": "-1000000000" }
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
        println!("Sui TX: {tx:?}");
        assert_eq!(
            &SuiExecutionStatus::Success,
            tx.effects.as_ref().unwrap().status()
        );
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
}

#[tokio::test]
async fn test_pay_with_many_small_coins() -> Result<()> {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?;
    assert!(
        !all_coins_sender.has_next_page,
        "Multiple pages for sender coins not implemented"
    );

    // Send rest of the coins to recipient first
    for coin in all_coins_sender.data.iter().skip(2) {
        let tx_data = client
            .transaction_builder()
            .transfer_object(
                sender,
                coin.coin_object_id,
                None,
                DEFAULT_GAS_BUDGET,
                recipient,
            )
            .await?;
        let sig = keystore.sign_secure(&sender, &tx_data, Intent::sui_transaction())?;
        let resp = client
            .quorum_driver_api()
            .execute_transaction_block(
                Transaction::from_data(tx_data, vec![sig]),
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_object_changes(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;
        assert!(
            resp.status_ok().is_some() && resp.status_ok().unwrap(),
            "Something went wrong sending coins"
        );
    }

    // First two coins were probably been used for gas already so update coins:
    let all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?;
    assert!(
        all_coins_sender.data.len() == 2,
        "Should have exactly 2 coins by now."
    );
    let mut iter = all_coins_sender.data.into_iter();
    let coin_to_split = iter.next().unwrap();
    let coin_for_gas = iter.next().unwrap();
    let new_coins = 300;
    let split_amount = coin_to_split.balance / new_coins;
    let amount_to_send = split_amount as i128 * 257;
    let recipient_change = amount_to_send.to_string();
    let sender_change = (-amount_to_send).to_string();

    // Split balance to something that will need more than 256 coins to execute:
    let resps = make_change(
        &client,
        keystore,
        sender,
        coin_to_split,
        Some(coin_for_gas.object_ref()),
        split_amount,
    )
    .await?;

    for resp in resps {
        assert!(
            resp.status_ok().is_some() && resp.status_ok().unwrap(),
            "Something went wrong splitting coins to change"
        );
    }

    // Now send coin previously been used as gas, in order to only have
    // the change coins.
    let tx_data = client
        .transaction_builder()
        .transfer_object(
            sender,
            coin_for_gas.coin_object_id,
            None,
            DEFAULT_GAS_BUDGET,
            recipient,
        )
        .await?;
    let sig = keystore.sign_secure(&sender, &tx_data, Intent::sui_transaction())?;
    let resp = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(tx_data, vec![sig]),
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await?;
    assert!(
        resp.status_ok().is_some() && resp.status_ok().unwrap(),
        "Something went wrong sending coins"
    );

    // Test rosetta can handle using many "small" coins for payment
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

    Ok(())
}
