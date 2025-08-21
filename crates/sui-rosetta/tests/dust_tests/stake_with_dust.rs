// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;

use anyhow::Result;
use serde_json::json;
use shared_crypto::intent::Intent;
use sui_json_rpc_types::{
    SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::{operations::Operations, CoinMetadataCache};
use sui_types::{quorum_driver_types::ExecuteTransactionRequestType, transaction::Transaction};
use test_cluster::TestClusterBuilder;

use crate::{
    rosetta_client::start_rosetta_test_server,
    split_coin::{make_change, DEFAULT_GAS_BUDGET},
};

#[tokio::test]
async fn test_stake_with_many_small_coins() -> Result<()> {
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
    let mut all_coins_iter = all_coins_sender.data.into_iter();
    let coin_to_split = all_coins_iter.next().unwrap();
    let _gas_for_split_tx = all_coins_iter.next().unwrap();

    let amount_to_stake: i128 = (coin_to_split.balance as i128 * 299) / 300; // Almost all coin to go for staking
    let split_amount = (amount_to_stake / 299) as u64; // Make sure we need more than 255 coins

    // Send rest of the coins to recipient first
    for coin in all_coins_iter {
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
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
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
    // Update references of existing coin objects
    let mut all_coins = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?
        .data;
    let (coin_to_split, gas_for_split_tx) = match all_coins.as_mut_slice() {
        [coin_0, _] => {
            if coin_0.coin_object_id == coin_to_split.coin_object_id {
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
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
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
async fn test_stake_with_multiple_merges() -> Result<()> {
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
    let mut all_coins_iter = all_coins_sender.data.into_iter();
    let coin_to_split = all_coins_iter.next().unwrap();
    let _gas_for_split_tx = all_coins_iter.next().unwrap();

    let amount_to_stake: i128 =
        (coin_to_split.balance as i128 * 255 + 511 + 100) / (255 + 511 + 101); // Almost all coin to go for staking
    let split_amount = amount_to_stake as u64 / (255 + 511 + 100); // Make sure we need more than 255 + 511 coins

    // Send rest of the coins to recipient first
    for coin in all_coins_iter {
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
        let sig = keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())
            .await?;
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
    // Update references of existing coin objects
    let mut all_coins = client
        .coin_read_api()
        .get_all_coins(sender, None, None)
        .await?
        .data;
    let (coin_to_split, gas_for_split_tx) = match all_coins.as_mut_slice() {
        [coin_0, _] => {
            if coin_0.coin_object_id == coin_to_split.coin_object_id {
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
    let sig = keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await?;
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
    let (rosetta_client, mut _handle) = start_rosetta_test_server(client.clone()).await;

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
