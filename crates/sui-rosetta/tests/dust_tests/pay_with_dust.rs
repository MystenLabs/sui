// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Result;
use once_cell::sync::Lazy;
use rosetta_client::{start_rosetta_test_server, RosettaError};
use serde_json::json;
use shared_crypto::intent::Intent;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockDataAPI, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::types::PreprocessMetadata;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::supported_protocol_versions::ProtocolConfig;
use sui_types::transaction::Transaction;
use test_cluster::TestClusterBuilder;

use super::split_coin::{make_change, DEFAULT_GAS_BUDGET};

static MAX_GAS_BUDGET: Lazy<u64> =
    Lazy::new(|| ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas());

#[allow(dead_code)]
#[path = "../rosetta_client.rs"]
mod rosetta_client;

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
    let gas_for_split_tx = iter.next().unwrap();
    let new_coins = 300;
    let split_amount = coin_to_split.balance / new_coins;
    let amount_to_send = split_amount as i128 * 257;
    let recipient_change = amount_to_send.to_string();
    let sender_change = (-amount_to_send).to_string();

    // Split balance to something that will need more than 255 coins to execute:
    let resps = make_change(
        &client,
        keystore,
        sender,
        coin_to_split,
        Some(gas_for_split_tx.object_ref()),
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
            gas_for_split_tx.coin_object_id,
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

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

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

// The limit actually passes for 1650 coins, but it often fails with
// "Failed to confirm tx status for TransactionDigest(...) within .. seconds.".
// This originates from the fact that we pass None as the ExecuteTransactionRequestType
// in the submit endpoint. This defaults to WaitForLocalExecution which has a timetout.
#[tokio::test]
async fn test_limit_many_small_coins() -> Result<()> {
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
    let gas_for_split_tx = iter.next().unwrap();
    let new_coins = 2048;
    let split_amount = coin_to_split.balance / new_coins;
    let amount_to_send = split_amount as i128 * 1500;
    let recipient_change = amount_to_send.to_string();
    let sender_change = (-amount_to_send).to_string();

    // Split balance to something that will need more than 255 coins to execute:
    let resps = make_change(
        &client,
        keystore,
        sender,
        coin_to_split,
        Some(gas_for_split_tx.object_ref()),
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
            gas_for_split_tx.coin_object_id,
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

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

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

#[tokio::test]
async fn test_pay_with_many_small_coins_with_budget() -> Result<()> {
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
    let gas_for_split_tx = iter.next().unwrap();
    let new_coins = 300;
    let split_amount = coin_to_split.balance / new_coins;
    let amount_to_send = split_amount as i128 * 257;
    let budget = u64::min(split_amount * (new_coins - 258), *MAX_GAS_BUDGET);
    let recipient_change = amount_to_send.to_string();
    let sender_change = (-amount_to_send).to_string();

    // Split balance to something that will need more than 255 coins to execute:
    let resps = make_change(
        &client,
        keystore,
        sender,
        coin_to_split,
        Some(gas_for_split_tx.object_ref()),
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
            gas_for_split_tx.coin_object_id,
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
    assert_eq!(tx.transaction.unwrap().data.gas_data().budget, budget);

    Ok(())
}

#[tokio::test]
async fn test_pay_with_many_small_coins_fail_insufficient_balance_budget_none() -> Result<()> {
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
    let mut all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?;
    assert!(
        all_coins_sender.data.len() == 2,
        "Should have exactly 2 coins by now."
    );

    // Keep one small coin
    let coin_to_send = all_coins_sender.data.pop().unwrap();
    let new_coins = 300;
    let split_amount = 3_000_000;
    let send_amount_to_decrease_balance = coin_to_send.balance - new_coins * split_amount;

    let tx_data = client
        .transaction_builder()
        .pay_sui(
            sender,
            vec![coin_to_send.coin_object_id],
            vec![recipient],
            vec![send_amount_to_decrease_balance],
            2_000_000,
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
    assert!(*resp.effects.unwrap().status() == SuiExecutionStatus::Success);

    // Update the coins state
    let mut all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?
        .data;

    // Differentiate between the two coins
    let coin_to_split_id = coin_to_send.coin_object_id;
    let (coin_to_split, gas_for_split_tx) = match all_coins_sender.as_mut_slice() {
        [coin_0, _] if coin_0.coin_object_id == coin_to_split_id => {
            let gas_for_split_tx = all_coins_sender.pop().unwrap();
            (all_coins_sender.pop().unwrap(), gas_for_split_tx)
        }
        [_, _] => (
            all_coins_sender.pop().unwrap(),
            all_coins_sender.pop().unwrap(),
        ),
        _ => unreachable!("Vector should have exactly two elements"),
    };
    let initial_balance = coin_to_split.balance;

    // Split balance to something that will need more than 255 coins to execute:
    let resps = make_change(
        &client,
        keystore,
        sender,
        coin_to_split,
        Some(gas_for_split_tx.object_ref()),
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
            gas_for_split_tx.coin_object_id,
            None,
            2_000_000,
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
    let tx_cost_summary = resp.effects.unwrap().gas_cost_summary().net_gas_usage();
    let total_amount = initial_balance as i128 - tx_cost_summary as i128;
    let expected_budget = 3_076_000; // Calculated after a successful dry-run
    let recipient_change = total_amount - expected_budget + 1; // Make it fail with insufficient
    let sender_change = expected_budget - total_amount - 1;

    // Test rosetta can handle using many "small" coins for payment
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

    println!(
        "{}",
        serde_json::to_string_pretty(&resps.preprocess.as_ref().unwrap().as_ref().unwrap())?
    );
    let Some(Err(err)) = resps.metadata else {
        panic!("Expected metadata to exists and error");
    };

    let mut expected_details = HashMap::new();
    expected_details.insert("error", format!("Invalid input: Address {sender} does not have enough Sui balance to transfer {recipient_change} with needed budget: {expected_budget}. Sui balance: {}.", recipient_change + expected_budget - 1));
    assert_eq!(
        err,
        RosettaError {
            code: 2,
            message: "Invalid input".to_string(),
            description: None,
            retriable: false,
            details: Some(serde_json::to_value(expected_details).unwrap())
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
    let mut all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?;
    assert!(
        all_coins_sender.data.len() == 2,
        "Should have exactly 2 coins by now."
    );

    // Keep one small coin
    let coin_to_send = all_coins_sender.data.pop().unwrap();
    let new_coins = 300;
    let split_amount = 3_000_000;
    let send_amount_to_decrease_balance = coin_to_send.balance - new_coins * split_amount;

    let tx_data = client
        .transaction_builder()
        .pay_sui(
            sender,
            vec![coin_to_send.coin_object_id],
            vec![recipient],
            vec![send_amount_to_decrease_balance],
            2_000_000,
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
    assert!(*resp.effects.unwrap().status() == SuiExecutionStatus::Success);

    // Update the coins state
    let mut all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?
        .data;

    // Differentiate between the two coins
    let coin_to_split_id = coin_to_send.coin_object_id;
    let (coin_to_split, gas_for_split_tx) = match all_coins_sender.as_mut_slice() {
        [coin_0, _] if coin_0.coin_object_id == coin_to_split_id => {
            let gas_for_split_tx = all_coins_sender.pop().unwrap();
            (all_coins_sender.pop().unwrap(), gas_for_split_tx)
        }
        [_, _] => (
            all_coins_sender.pop().unwrap(),
            all_coins_sender.pop().unwrap(),
        ),
        _ => unreachable!("Vector should have exactly two elements"),
    };
    let initial_balance = coin_to_split.balance;

    // Split balance to something that will need more than 255 coins to execute:
    let resps = make_change(
        &client,
        keystore,
        sender,
        coin_to_split,
        Some(gas_for_split_tx.object_ref()),
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
            gas_for_split_tx.coin_object_id,
            None,
            2_000_000,
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
    let tx_cost_summary = resp.effects.unwrap().gas_cost_summary().net_gas_usage();
    let total_amount = initial_balance as i128 - tx_cost_summary as i128;
    let budget = 3_076_000; // Calculated from successful dry-run
    let recipient_change = total_amount - budget;
    let sender_change = budget - total_amount;

    // Test rosetta can handle using many "small" coins for payment
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

    let details = Some(
        serde_json::to_value(HashMap::from([(
            "error",
            format!(
                "Insufficient fund for address [{sender}], requested amount: {}",
                recipient_change + budget + 1
            ),
        )]))
        .unwrap(),
    );
    let Some(Err(e)) = resps.metadata else {
        panic!("Expected metadata to exist and error")
    };
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
async fn test_pay_with_many_small_coins_fail_insufficient_budget() -> Result<()> {
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
    let mut all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?;
    assert!(
        all_coins_sender.data.len() == 2,
        "Should have exactly 2 coins by now."
    );

    // Keep one small coin
    let coin_to_send = all_coins_sender.data.pop().unwrap();
    let new_coins = 300;
    let split_amount = 3_000_000;
    let send_amount_to_decrease_balance = coin_to_send.balance - new_coins * split_amount;

    let tx_data = client
        .transaction_builder()
        .pay_sui(
            sender,
            vec![coin_to_send.coin_object_id],
            vec![recipient],
            vec![send_amount_to_decrease_balance],
            2_000_000,
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
    assert!(*resp.effects.unwrap().status() == SuiExecutionStatus::Success);

    // Update the coins state
    let mut all_coins_sender = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?
        .data;

    // Differentiate between the two coins
    let coin_to_split_id = coin_to_send.coin_object_id;
    let (coin_to_split, gas_for_split_tx) = match all_coins_sender.as_mut_slice() {
        [coin_0, _] if coin_0.coin_object_id == coin_to_split_id => {
            let gas_for_split_tx = all_coins_sender.pop().unwrap();
            (all_coins_sender.pop().unwrap(), gas_for_split_tx)
        }
        [_, _] => (
            all_coins_sender.pop().unwrap(),
            all_coins_sender.pop().unwrap(),
        ),
        _ => unreachable!("Vector should have exactly two elements"),
    };
    let initial_balance = coin_to_split.balance;

    // Split balance to something that will need more than 255 coins to execute:
    let resps = make_change(
        &client,
        keystore,
        sender,
        coin_to_split,
        Some(gas_for_split_tx.object_ref()),
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
            gas_for_split_tx.coin_object_id,
            None,
            2_000_000,
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
    let tx_cost_summary = resp.effects.unwrap().gas_cost_summary().net_gas_usage();
    let total_amount = initial_balance as i128 - tx_cost_summary as i128;
    let budget = 3_000_000; // This is actually less than what the tx requires
    let recipient_change = total_amount - budget;
    let sender_change = budget - total_amount;

    // Test rosetta can handle using many "small" coins for payment
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

    let _rosetta_resp = rosetta_client
        .rosetta_flow(
            &ops,
            keystore,
            Some(PreprocessMetadata {
                budget: Some(budget as u64),
            }),
        )
        .await;

    // TODO: Actually budget required is max(computation_cost, computation_cost + storage_cost - storage_rebate)
    panic!("This should have failed as gas budget is smaller than what the tx requires");
}
