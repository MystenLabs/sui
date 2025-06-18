// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_rpc_api::proto::rpc::v2beta2::live_data_service_client::LiveDataServiceClient;
use sui_rpc_api::proto::rpc::v2beta2::Argument;
use sui_rpc_api::proto::rpc::v2beta2::Command;
use sui_rpc_api::proto::rpc::v2beta2::GasPayment;
use sui_rpc_api::proto::rpc::v2beta2::Input;
use sui_rpc_api::proto::rpc::v2beta2::MoveCall;
use sui_rpc_api::proto::rpc::v2beta2::ProgrammableTransaction;
use sui_rpc_api::proto::rpc::v2beta2::SimulateTransactionRequest;
use sui_rpc_api::proto::rpc::v2beta2::Transaction;
use sui_rpc_api::proto::rpc::v2beta2::TransactionKind;
use sui_rpc_api::proto::rpc::v2beta2::TransferObjects;
use sui_rpc_api::Client;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::TestClusterBuilder;

fn proto_to_response(
    proto: sui_rpc_api::proto::rpc::v2beta2::SimulateTransactionResponse,
) -> (
    sui_types::transaction::TransactionData,
    sui_types::effects::TransactionEffects,
    Option<sui_types::effects::TransactionEvents>,
) {
    let executed_transaction = proto.transaction.unwrap();
    let transaction = executed_transaction
        .transaction
        .unwrap()
        .bcs
        .unwrap()
        .deserialize()
        .unwrap();
    let effects = executed_transaction
        .effects
        .unwrap()
        .bcs
        .unwrap()
        .deserialize()
        .unwrap();
    let events = executed_transaction
        .events
        .map(|events| events.bcs.unwrap().deserialize().unwrap());

    (transaction, effects, events)
}

#[sim_test]
async fn resolve_transaction_simple_transfer() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let recipient = SuiAddress::random_for_testing_only();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_send = gas.first().unwrap().0;

    let unresolved_transaction = Transaction {
        kind: Some(TransactionKind::from(ProgrammableTransaction {
            inputs: vec![
                Input {
                    object_id: Some(obj_to_send.to_canonical_string(true)),
                    ..Default::default()
                },
                Input {
                    literal: Some(Box::new(recipient.to_string().into())),
                    ..Default::default()
                },
            ],
            commands: vec![Command::from(TransferObjects {
                objects: vec![Argument::new_input(0)],
                address: Some(Argument::new_input(1)),
            })],
        })),
        sender: Some(sender.to_string()),
        ..Default::default()
    };

    let resolved = alpha_client
        .simulate_transaction(SimulateTransactionRequest {
            transaction: Some(unresolved_transaction),
            do_gas_selection: Some(true),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let (transaction, effects_from_simulation, _events) = proto_to_response(resolved);

    let signed_transaction = test_cluster.wallet.sign_transaction(&transaction);
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(effects_from_simulation, effects);
}

#[sim_test]
async fn resolve_transaction_transfer_with_sponsor() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let recipient = SuiAddress::random_for_testing_only();

    let (sender, gas) = test_cluster.wallet.get_one_account().await.unwrap();
    let obj_to_send = gas.first().unwrap().0;
    let sponsor = test_cluster.wallet.get_addresses()[1];

    let unresolved_transaction = Transaction {
        kind: Some(TransactionKind::from(ProgrammableTransaction {
            inputs: vec![
                Input {
                    object_id: Some(obj_to_send.to_canonical_string(true)),
                    ..Default::default()
                },
                Input {
                    literal: Some(Box::new(recipient.to_string().into())),
                    ..Default::default()
                },
            ],
            commands: vec![Command::from(TransferObjects {
                objects: vec![Argument::new_input(0)],
                address: Some(Argument::new_input(1)),
            })],
        })),
        sender: Some(sender.to_string()),
        gas_payment: Some(GasPayment {
            owner: Some(sponsor.to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };

    let resolved = alpha_client
        .simulate_transaction(SimulateTransactionRequest {
            transaction: Some(unresolved_transaction),
            do_gas_selection: Some(true),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let (transaction, effects_from_simulation, _events) = proto_to_response(resolved);

    let sender_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &transaction, Intent::sui_transaction())
        .unwrap();
    let sponsor_sig = test_cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &transaction, Intent::sui_transaction())
        .unwrap();

    let signed_transaction =
        sui_types::transaction::Transaction::from_data(transaction, vec![sender_sig, sponsor_sig]);
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(effects_from_simulation, effects);
}

#[sim_test]
async fn resolve_transaction_borrowed_shared_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let sender = test_cluster.wallet.get_addresses()[0];

    let unresolved_transaction = Transaction {
        kind: Some(TransactionKind::from(ProgrammableTransaction {
            inputs: vec![Input {
                object_id: Some("0x6".to_owned()),
                ..Default::default()
            }],
            commands: vec![Command::from(MoveCall {
                package: Some("0x2".to_owned()),
                module: Some("clock".to_owned()),
                function: Some("timestamp_ms".to_owned()),
                type_arguments: vec![],
                arguments: vec![Argument::new_input(0)],
            })],
        })),
        sender: Some(sender.to_string()),
        ..Default::default()
    };

    let resolved = alpha_client
        .simulate_transaction(SimulateTransactionRequest {
            transaction: Some(unresolved_transaction),
            do_gas_selection: Some(true),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let (transaction, _effects, _events) = proto_to_response(resolved);

    let signed_transaction = test_cluster.wallet.sign_transaction(&transaction);
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
}

#[sim_test]
async fn resolve_transaction_mutable_shared_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_stake = gas.first().unwrap().0;

    let validator_address = test_cluster.swarm.config().validator_configs()[0].sui_address();

    let unresolved_transaction = Transaction {
        kind: Some(TransactionKind::from(ProgrammableTransaction {
            inputs: vec![
                Input {
                    object_id: Some("0x5".to_owned()),
                    ..Default::default()
                },
                Input {
                    object_id: Some(obj_to_stake.to_canonical_string(true)),
                    ..Default::default()
                },
                Input {
                    literal: Some(Box::new(validator_address.to_string().into())),
                    ..Default::default()
                },
            ],
            commands: vec![Command::from(MoveCall {
                package: Some("0x3".to_owned()),
                module: Some("sui_system".to_owned()),
                function: Some("request_add_stake".to_owned()),
                type_arguments: vec![],
                arguments: vec![
                    Argument::new_input(0),
                    Argument::new_input(1),
                    Argument::new_input(2),
                ],
            })],
        })),
        sender: Some(sender.to_string()),
        ..Default::default()
    };

    let resolved = alpha_client
        .simulate_transaction(SimulateTransactionRequest {
            transaction: Some(unresolved_transaction),
            do_gas_selection: Some(true),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    let (transaction, effects_from_simulation, _events) = proto_to_response(resolved);

    let signed_transaction = test_cluster.wallet.sign_transaction(&transaction);
    let effects = client
        .execute_transaction(&signed_transaction)
        .await
        .unwrap()
        .effects;

    assert!(effects.status().is_ok());
    assert_eq!(effects_from_simulation, effects);
}

#[sim_test]
async fn resolve_transaction_insufficient_gas() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut alpha_client = LiveDataServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Test the case where we don't have enough coins/gas for the required budget
    let unresolved_transaction = Transaction {
        kind: Some(TransactionKind::from(ProgrammableTransaction {
            inputs: vec![Input {
                object_id: Some("0x6".to_owned()),
                ..Default::default()
            }],
            commands: vec![Command::from(MoveCall {
                package: Some("0x2".to_owned()),
                module: Some("clock".to_owned()),
                function: Some("timestamp_ms".to_owned()),
                type_arguments: vec![],
                arguments: vec![Argument::new_input(0)],
            })],
        })),
        sender: Some(SuiAddress::random_for_testing_only().to_string()), // random account with no
        // gas
        ..Default::default()
    };

    let error = alpha_client
        .simulate_transaction(SimulateTransactionRequest {
            transaction: Some(unresolved_transaction),
            do_gas_selection: Some(true),
            ..Default::default()
        })
        .await
        .unwrap_err();

    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert_contains(error.message(), "unable to select sufficient gas");
}

fn assert_contains(haystack: &str, needle: &str) {
    if !haystack.contains(needle) {
        panic!("{haystack:?} does not contain {needle:?}");
    }
}
