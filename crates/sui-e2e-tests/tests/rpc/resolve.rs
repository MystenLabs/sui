// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_rpc_api::client::reqwest::StatusCode;
use sui_rpc_api::client::sdk::Client as RestClient;
use sui_rpc_api::rest::transactions::ResolveTransactionQueryParameters;
use sui_rpc_api::Client;
use sui_sdk_types::types::unresolved;
use sui_sdk_types::types::Argument;
use sui_sdk_types::types::Command;
use sui_sdk_types::types::TransactionExpiration;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn resolve_transaction_simple_transfer() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let rest_client = RestClient::new(test_cluster.rpc_url()).unwrap();
    let recipient = SuiAddress::random_for_testing_only();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_send = gas.first().unwrap().0;

    let unresolved_transaction = unresolved::Transaction {
        ptb: unresolved::ProgrammableTransaction {
            inputs: vec![
                unresolved::Input {
                    object_id: Some(obj_to_send.into()),
                    ..Default::default()
                },
                unresolved::Input {
                    value: Some(unresolved::Value::String(recipient.to_string())),
                    ..Default::default()
                },
            ],
            commands: vec![Command::TransferObjects(
                sui_sdk_types::types::TransferObjects {
                    objects: vec![Argument::Input(0)],
                    address: Argument::Input(1),
                },
            )],
        },
        sender: sender.into(),
        gas_payment: None,
        expiration: TransactionExpiration::None,
    };

    let resolved = rest_client
        .resolve_transaction_with_parameters(
            &unresolved_transaction,
            &ResolveTransactionQueryParameters {
                simulate: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .into_inner();

    let signed_transaction = test_cluster
        .wallet
        .sign_transaction(&resolved.transaction.try_into().unwrap());
    let effects = client
        .execute_transaction(&Default::default(), &signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(
        resolved.simulation.unwrap().effects,
        effects.try_into().unwrap()
    );
}

#[sim_test]
async fn resolve_transaction_transfer_with_sponsor() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let rest_client = RestClient::new(test_cluster.rpc_url()).unwrap();
    let recipient = SuiAddress::random_for_testing_only();

    let (sender, gas) = test_cluster.wallet.get_one_account().await.unwrap();
    let obj_to_send = gas.first().unwrap().0;
    let sponsor = test_cluster.wallet.get_addresses()[1];

    let unresolved_transaction = unresolved::Transaction {
        ptb: unresolved::ProgrammableTransaction {
            inputs: vec![
                unresolved::Input {
                    object_id: Some(obj_to_send.into()),
                    ..Default::default()
                },
                unresolved::Input {
                    value: Some(unresolved::Value::String(recipient.to_string())),
                    ..Default::default()
                },
            ],
            commands: vec![Command::TransferObjects(
                sui_sdk_types::types::TransferObjects {
                    objects: vec![Argument::Input(0)],
                    address: Argument::Input(1),
                },
            )],
        },
        sender: sender.into(),
        gas_payment: Some(unresolved::GasPayment {
            objects: vec![],
            owner: sponsor.into(),
            price: None,
            budget: None,
        }),
        expiration: TransactionExpiration::None,
    };

    let resolved = rest_client
        .resolve_transaction_with_parameters(
            &unresolved_transaction,
            &ResolveTransactionQueryParameters {
                simulate: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .into_inner();

    let transaction_data = resolved.transaction.clone().try_into().unwrap();
    let sender_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &transaction_data, Intent::sui_transaction())
        .unwrap();
    let sponsor_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &transaction_data, Intent::sui_transaction())
        .unwrap();

    let signed_transaction = sui_types::transaction::Transaction::from_data(
        transaction_data,
        vec![sender_sig, sponsor_sig],
    );
    let effects = client
        .execute_transaction(&Default::default(), &signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(
        resolved.simulation.unwrap().effects,
        effects.try_into().unwrap()
    );
}

#[sim_test]
async fn resolve_transaction_borrowed_shared_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let rest_client = RestClient::new(test_cluster.rpc_url()).unwrap();

    let sender = test_cluster.wallet.get_addresses()[0];

    let unresolved_transaction = unresolved::Transaction {
        ptb: unresolved::ProgrammableTransaction {
            inputs: vec![unresolved::Input {
                object_id: Some("0x6".parse().unwrap()),
                ..Default::default()
            }],
            commands: vec![Command::MoveCall(sui_sdk_types::types::MoveCall {
                package: "0x2".parse().unwrap(),
                module: "clock".parse().unwrap(),
                function: "timestamp_ms".parse().unwrap(),
                type_arguments: vec![],
                arguments: vec![Argument::Input(0)],
            })],
        },
        sender: sender.into(),
        gas_payment: None,
        expiration: TransactionExpiration::None,
    };

    let resolved = rest_client
        .resolve_transaction_with_parameters(
            &unresolved_transaction,
            &ResolveTransactionQueryParameters {
                simulate: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .into_inner();

    let signed_transaction = test_cluster
        .wallet
        .sign_transaction(&resolved.transaction.try_into().unwrap());
    let effects = client
        .execute_transaction(&Default::default(), &signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
}

#[sim_test]
async fn resolve_transaction_mutable_shared_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let rest_client = RestClient::new(test_cluster.rpc_url()).unwrap();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_stake = gas.first().unwrap().0;
    let validator_address = rest_client
        .get_system_state_summary()
        .await
        .unwrap()
        .inner()
        .active_validators
        .first()
        .unwrap()
        .address;

    let unresolved_transaction = unresolved::Transaction {
        ptb: unresolved::ProgrammableTransaction {
            inputs: vec![
                unresolved::Input {
                    object_id: Some("0x5".parse().unwrap()),
                    ..Default::default()
                },
                unresolved::Input {
                    object_id: Some(obj_to_stake.into()),
                    ..Default::default()
                },
                unresolved::Input {
                    value: Some(unresolved::Value::String(validator_address.to_string())),
                    ..Default::default()
                },
            ],
            commands: vec![Command::MoveCall(sui_sdk_types::types::MoveCall {
                package: "0x3".parse().unwrap(),
                module: "sui_system".parse().unwrap(),
                function: "request_add_stake".parse().unwrap(),
                type_arguments: vec![],
                arguments: vec![Argument::Input(0), Argument::Input(1), Argument::Input(2)],
            })],
        },
        sender: sender.into(),
        gas_payment: None,
        expiration: TransactionExpiration::None,
    };

    let resolved = rest_client
        .resolve_transaction_with_parameters(
            &unresolved_transaction,
            &ResolveTransactionQueryParameters {
                simulate: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .into_inner();

    let signed_transaction = test_cluster
        .wallet
        .sign_transaction(&resolved.transaction.try_into().unwrap());
    let effects = client
        .execute_transaction(&Default::default(), &signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(
        resolved.simulation.unwrap().effects,
        effects.try_into().unwrap()
    );
}

#[sim_test]
async fn resolve_transaction_insufficient_gas() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let rest_client = RestClient::new(test_cluster.rpc_url()).unwrap();

    // Test the case where we don't have enough coins/gas for the required budget
    let unresolved_transaction = unresolved::Transaction {
        ptb: unresolved::ProgrammableTransaction {
            inputs: vec![unresolved::Input {
                object_id: Some("0x6".parse().unwrap()),
                ..Default::default()
            }],
            commands: vec![Command::MoveCall(sui_sdk_types::types::MoveCall {
                package: "0x2".parse().unwrap(),
                module: "clock".parse().unwrap(),
                function: "timestamp_ms".parse().unwrap(),
                type_arguments: vec![],
                arguments: vec![Argument::Input(0)],
            })],
        },
        sender: SuiAddress::random_for_testing_only().into(), // random account with no gas
        gas_payment: None,
        expiration: TransactionExpiration::None,
    };

    let error = rest_client
        .resolve_transaction(&unresolved_transaction)
        .await
        .unwrap_err();

    assert_eq!(error.status(), Some(StatusCode::BAD_REQUEST));
    assert_contains(
        error.message().unwrap_or_default(),
        "unable to select sufficient gas",
    );
}

fn assert_contains(haystack: &str, needle: &str) {
    if !haystack.contains(needle) {
        panic!("{haystack:?} does not contain {needle:?}");
    }
}

#[sim_test]
async fn resolve_transaction_with_raw_json() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let rest_client = RestClient::new(test_cluster.rpc_url()).unwrap();
    let recipient = SuiAddress::random_for_testing_only();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_send = gas.first().unwrap().0;

    let unresolved_transaction = serde_json::json!({
        "inputs": [
            {
                "object_id": obj_to_send
            },
            {
                "value": 1
            },
            {
                "value": recipient
            }
        ],

        "commands": [
            {
                "command": "split_coins",
                "coin": { "input": 0 },
                "amounts": [
                    {
                        "input": 1,
                    },
                    {
                        "input": 1,
                    }
                ]
            },
            {
                "command": "transfer_objects",
                "objects": [
                    { "result": [0, 1] },
                    { "result": [0, 0] }
                ],
                "address": { "input": 2 }
            }
        ],

        "sender": sender
    });

    let resolved = rest_client
        .resolve_transaction_with_parameters(
            &serde_json::from_value(unresolved_transaction).unwrap(),
            &ResolveTransactionQueryParameters {
                simulate: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .into_inner();

    let signed_transaction = test_cluster
        .wallet
        .sign_transaction(&resolved.transaction.try_into().unwrap());
    let effects = client
        .execute_transaction(&Default::default(), &signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_eq!(
        resolved.simulation.unwrap().effects,
        effects.try_into().unwrap()
    );
}
