// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use rand::rngs::OsRng;
use rand::seq::IteratorRandom;
use rosetta_client::start_rosetta_test_server;
use serde_json::json;
use shared_crypto::intent::Intent;
use std::num::NonZeroUsize;
use std::time::Duration;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponseQuery, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Currency, NetworkIdentifier,
    SubAccount, SubAccountType, SuiEnv,
};
use sui_rosetta::types::{Currencies, OperationType, TransactionIdentifierResponse};
use sui_rosetta::CoinMetadataCache;
use sui_sdk::rpc_types::{SuiExecutionStatus, SuiTransactionBlockEffectsAPI};
use sui_sdk::SuiClient;
use sui_swarm_config::genesis_config::{DEFAULT_GAS_AMOUNT, DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{
    Argument, InputObjectKind, Transaction, TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::to_sender_signed_transaction;
use test_cluster::TestClusterBuilder;

use crate::rosetta_client::RosettaEndpoint;

#[allow(dead_code)]
mod rosetta_client;

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
        .await
        .unwrap();
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
        .await
        .unwrap();
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

    let response: TransactionIdentifierResponse = rosetta_client
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

async fn get_random_sui(
    client: &SuiClient,
    sender: SuiAddress,
    except: Vec<ObjectID>,
) -> ObjectRef {
    let coins = client
        .read_api()
        .get_owned_objects(
            sender,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            /* cursor */ None,
            /* limit */ None,
        )
        .await
        .unwrap()
        .data;

    let coin_resp = coins
        .iter()
        .filter(|object| {
            let obj = object.object().unwrap();
            obj.is_gas_coin() && !except.contains(&obj.object_id)
        })
        .choose(&mut OsRng)
        .unwrap();

    let coin = coin_resp.object().unwrap();
    (coin.object_id, coin.version, coin.digest)
}
#[tokio::test]
async fn test_transfer_single_gas_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let client = test_cluster.wallet.get_client().await.unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_arg(recipient, Argument::GasCoin);
        builder.finish()
    };

    let input_objects = pt
        .input_objects()
        .unwrap_or_default()
        .iter()
        .flat_map(|obj| {
            if let InputObjectKind::ImmOrOwnedMoveObject((id, ..)) = obj {
                Some(*id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let gas = vec![get_random_sui(&client, sender, input_objects).await];
    let gas_price = client
        .governance_api()
        .get_reference_gas_price()
        .await
        .unwrap();

    let data = TransactionData::new_programmable(
        sender,
        gas,
        pt,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        gas_price,
    );

    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .unwrap();

    let response = client
        .quorum_driver_api()
        .execute_transaction_block(
            Transaction::from_data(data.clone(), vec![signature]),
            SuiTransactionBlockResponseOptions::new()
                .with_effects()
                .with_object_changes()
                .with_balance_changes()
                .with_input(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .map_err(|e| anyhow!("TX execution failed for {data:#?}, error : {e}"))
        .unwrap();

    let coin_cache = CoinMetadataCache::new(client, NonZeroUsize::new(2).unwrap());
    let operations = Operations::try_from_response(response, &coin_cache)
        .await
        .unwrap();
    println!("operations: {operations:#?}");

    let mut balance = 0;
    operations
        .into_iter()
        .for_each(|op| {
            if op.type_ == OperationType::Gas {
                assert_eq!(op.account.unwrap().address, sender);
            }
            if op.type_ == OperationType::PaySui {
                balance += op.amount.unwrap().value;
            }
        });
    assert_eq!(balance, 0);
}
