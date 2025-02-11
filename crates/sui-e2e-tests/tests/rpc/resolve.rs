// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost_types::FieldMask;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_rpc_api::proto::node::v2alpha::ResolveTransactionRequest;
use sui_rpc_api::types::ResolveTransactionResponse;
use sui_rpc_api::types::TransactionSimulationResponse;
use sui_rpc_api::Client;
use sui_sdk_transaction_builder::unresolved;
use sui_sdk_types::Argument;
use sui_sdk_types::Command;
use sui_sdk_types::TransactionExpiration;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::TestClusterBuilder;

fn build_resolve_request(
    transaction: &unresolved::Transaction,
    simulate: bool,
) -> ResolveTransactionRequest {
    let read_mask = if simulate {
        Some(FieldMask {
            paths: vec!["simulation".to_string()],
        })
    } else {
        None
    };
    ResolveTransactionRequest {
        unresolved_transaction: Some(serde_json::to_string(transaction).unwrap()),
        read_mask,
    }
}

fn proto_to_response(
    proto: sui_rpc_api::proto::node::v2alpha::ResolveTransactionResponse,
) -> ResolveTransactionResponse {
    ResolveTransactionResponse {
        transaction: proto.transaction_bcs.unwrap().deserialize().unwrap(),
        simulation: proto
            .simulation
            .map(|simulation| TransactionSimulationResponse {
                effects: simulation.effects_bcs.unwrap().deserialize().unwrap(),
                events: simulation
                    .events_bcs
                    .map(|events_bcs| events_bcs.deserialize().unwrap()),
                balance_changes: None,
                input_objects: None,
                output_objects: None,
            }),
    }
}

#[sim_test]
async fn resolve_transaction_simple_transfer() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut alpha_client =
        sui_rpc_api::proto::node::v2alpha::node_service_client::NodeServiceClient::connect(
            test_cluster.rpc_url().to_owned(),
        )
        .await
        .unwrap();
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
            commands: vec![Command::TransferObjects(sui_sdk_types::TransferObjects {
                objects: vec![Argument::Input(0)],
                address: Argument::Input(1),
            })],
        },
        sender: sender.into(),
        gas_payment: None,
        expiration: TransactionExpiration::None,
    };

    let resolved = alpha_client
        .resolve_transaction(build_resolve_request(&unresolved_transaction, true))
        .await
        .unwrap()
        .into_inner();
    let resolved = proto_to_response(resolved);

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
    let mut alpha_client =
        sui_rpc_api::proto::node::v2alpha::node_service_client::NodeServiceClient::connect(
            test_cluster.rpc_url().to_owned(),
        )
        .await
        .unwrap();
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
            commands: vec![Command::TransferObjects(sui_sdk_types::TransferObjects {
                objects: vec![Argument::Input(0)],
                address: Argument::Input(1),
            })],
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

    let resolved = alpha_client
        .resolve_transaction(build_resolve_request(&unresolved_transaction, true))
        .await
        .unwrap()
        .into_inner();
    let resolved = proto_to_response(resolved);

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
    let mut alpha_client =
        sui_rpc_api::proto::node::v2alpha::node_service_client::NodeServiceClient::connect(
            test_cluster.rpc_url().to_owned(),
        )
        .await
        .unwrap();

    let sender = test_cluster.wallet.get_addresses()[0];

    let unresolved_transaction = unresolved::Transaction {
        ptb: unresolved::ProgrammableTransaction {
            inputs: vec![unresolved::Input {
                object_id: Some("0x6".parse().unwrap()),
                ..Default::default()
            }],
            commands: vec![Command::MoveCall(sui_sdk_types::MoveCall {
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

    let resolved = alpha_client
        .resolve_transaction(build_resolve_request(&unresolved_transaction, true))
        .await
        .unwrap()
        .into_inner();
    let resolved = proto_to_response(resolved);

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
    let mut alpha_client =
        sui_rpc_api::proto::node::v2alpha::node_service_client::NodeServiceClient::connect(
            test_cluster.rpc_url().to_owned(),
        )
        .await
        .unwrap();

    let (sender, mut gas) = test_cluster.wallet.get_one_account().await.unwrap();
    gas.sort_by_key(|object_ref| object_ref.0);
    let obj_to_stake = gas.first().unwrap().0;

    let validator_address = test_cluster.swarm.config().validator_configs()[0].sui_address();

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
            commands: vec![Command::MoveCall(sui_sdk_types::MoveCall {
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

    let resolved = alpha_client
        .resolve_transaction(build_resolve_request(&unresolved_transaction, true))
        .await
        .unwrap()
        .into_inner();
    let resolved = proto_to_response(resolved);

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
    let mut alpha_client =
        sui_rpc_api::proto::node::v2alpha::node_service_client::NodeServiceClient::connect(
            test_cluster.rpc_url().to_owned(),
        )
        .await
        .unwrap();

    // Test the case where we don't have enough coins/gas for the required budget
    let unresolved_transaction = unresolved::Transaction {
        ptb: unresolved::ProgrammableTransaction {
            inputs: vec![unresolved::Input {
                object_id: Some("0x6".parse().unwrap()),
                ..Default::default()
            }],
            commands: vec![Command::MoveCall(sui_sdk_types::MoveCall {
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

    let error = alpha_client
        .resolve_transaction(build_resolve_request(&unresolved_transaction, false))
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
