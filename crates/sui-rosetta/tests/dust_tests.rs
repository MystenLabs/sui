use anyhow::Result;
use rosetta_client::start_rosetta_test_server;
use serde_json::json;
use shared_crypto::intent::Intent;
use split_coin::{make_change, DEFAULT_GAS_BUDGET};
use sui_json_rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponseOptions};
use sui_keys::keystore::AccountKeystore;
use sui_types::{quorum_driver_types::ExecuteTransactionRequestType, transaction::Transaction};
use test_cluster::TestClusterBuilder;

#[allow(dead_code)]
mod rosetta_client;
#[path = "dust_tests/split_coin.rs"]
mod split_coin;

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

    // Split balance to something that will need more than 255 coins to execute:
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
    let coin_for_gas = iter.next().unwrap();
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
