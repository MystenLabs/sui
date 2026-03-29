// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use prost_types::FieldMask;
use sui_macros::sim_test;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ExecuteTransactionResponse;
use sui_rpc::proto::sui::rpc::v2::Transaction;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_sdk_types::BalanceChange;
use sui_test_transaction_builder::{TestTransactionBuilder, make_transfer_sui_transaction};
use sui_types::base_types::SuiAddress;
use sui_types::transaction::TransactionDataAPI;
use test_cluster::TestClusterBuilder;

mod resolve;

#[sim_test]
async fn execute_transaction_transfer() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut client = TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let address = SuiAddress::random_for_testing_only();
    let amount = 9;

    let txn =
        make_transfer_sui_transaction(&test_cluster.wallet, Some(address), Some(amount)).await;
    let sender = txn.transaction_data().sender();

    let ExecuteTransactionResponse { transaction, .. } = client
        .execute_transaction({
            let mut message = ExecuteTransactionRequest::default();
            message.transaction = Some({
                let mut message = Transaction::default();
                message.bcs = Some(Bcs::serialize(txn.transaction_data()).unwrap());
                message
            });
            message.signatures = txn
                .tx_signatures()
                .iter()
                .map(|s| {
                    let mut message = UserSignature::default();
                    message.bcs = Some(Bcs::from(s.as_ref().to_owned()));
                    message
                })
                .collect();
            message.read_mask = Some(FieldMask::from_paths(["*"]));
            message
        })
        .await
        .unwrap()
        .into_inner();

    let transaction = transaction.unwrap();
    let gas_summary =
        sui_sdk_types::GasCostSummary::try_from(&transaction.effects.unwrap().gas_used.unwrap())
            .unwrap();
    let gas = gas_summary.net_gas_usage();

    let coin_type = sui_types::sui_sdk_types_conversions::type_tag_core_to_sdk(
        sui_types::gas_coin::GAS::type_tag(),
    )
    .unwrap();
    let mut expected = vec![
        BalanceChange {
            address: sender.into(),
            coin_type: coin_type.clone(),
            amount: -(amount as i128 + gas as i128),
        },
        BalanceChange {
            address: address.into(),
            coin_type,
            amount: amount as i128,
        },
    ];
    expected.sort_by_key(|e| e.address);

    let mut actual = transaction
        .balance_changes
        .into_iter()
        .map(|bc| BalanceChange::try_from(&bc).unwrap())
        .collect::<Vec<_>>();
    actual.sort_by_key(|e| e.address);

    assert_eq!(actual, expected);
}

/// Test that event JSON is rendered correctly for events emitted by a newly published package.
///
/// This is a regression test for a bug where event JSON rendering would fail with a LINKER_ERROR
/// when the event type was defined in a package that was just published in the same transaction.
/// The fix was to use an overlay package store that checks output_objects before the backing store.
#[sim_test]
async fn execute_transaction_publish_with_event_json() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut client = TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Build a publish transaction for the move_test_code package which includes
    // an init_with_event module that emits an Event during init
    let (sender, gas_object) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");

    let txn = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish_async(path)
                .await
                .build(),
        )
        .await;

    let ExecuteTransactionResponse { transaction, .. } = client
        .execute_transaction({
            let mut message = ExecuteTransactionRequest::default();
            message.transaction = Some({
                let mut message = Transaction::default();
                message.bcs = Some(Bcs::serialize(txn.transaction_data()).unwrap());
                message
            });
            message.signatures = txn
                .tx_signatures()
                .iter()
                .map(|s| {
                    let mut message = UserSignature::default();
                    message.bcs = Some(Bcs::from(s.as_ref().to_owned()));
                    message
                })
                .collect();
            // Request all fields including events with JSON
            message.read_mask = Some(FieldMask::from_paths(["*"]));
            message
        })
        .await
        .unwrap()
        .into_inner();

    let transaction = transaction.unwrap();
    let events = transaction.events.unwrap();

    // The init_with_event module emits exactly one event
    assert!(!events.events.is_empty(), "Expected at least one event");

    // Verify that the event JSON was rendered (not None)
    // Without the overlay package store fix, this would be None because the package
    // wouldn't be found in the backing store yet
    let event = &events.events[0];
    assert!(
        event.json.is_some(),
        "Event JSON should be rendered for events from newly published packages. \
         This is a regression - the LINKER_ERROR fix may not be working."
    );
}

/// Test that object JSON is rendered correctly for objects created by a newly published package.
///
/// This is a regression test for a bug where object JSON rendering would fail with a LINKER_ERROR
/// when the object type was defined in a package that was just published in the same transaction.
#[sim_test]
async fn execute_transaction_publish_with_object_json() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut client = TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Build a publish transaction for the move_test_code package which includes
    // an init_with_object module that creates a MyObject during init
    let (sender, gas_object) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");

    let txn = test_cluster
        .wallet
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish_async(path)
                .await
                .build(),
        )
        .await;

    let ExecuteTransactionResponse { transaction, .. } = client
        .execute_transaction({
            let mut message = ExecuteTransactionRequest::default();
            message.transaction = Some({
                let mut message = Transaction::default();
                message.bcs = Some(Bcs::serialize(txn.transaction_data()).unwrap());
                message
            });
            message.signatures = txn
                .tx_signatures()
                .iter()
                .map(|s| {
                    let mut message = UserSignature::default();
                    message.bcs = Some(Bcs::from(s.as_ref().to_owned()));
                    message
                })
                .collect();
            // Request all fields including objects with JSON
            message.read_mask = Some(FieldMask::from_paths(["*"]));
            message
        })
        .await
        .unwrap()
        .into_inner();

    let transaction = transaction.unwrap();
    let objects = transaction.objects.unwrap();

    // Find the MyObject created by the init_with_object module
    // It should have JSON rendered even though the package was just published
    let my_object = objects
        .objects
        .iter()
        .find(|obj| obj.object_type().contains("MyObject"))
        .expect("Expected to find MyObject in output objects");

    assert!(
        my_object.json.is_some(),
        "Object JSON should be rendered for objects from newly published packages. \
         This is a regression - the LINKER_ERROR fix may not be working."
    );
}
