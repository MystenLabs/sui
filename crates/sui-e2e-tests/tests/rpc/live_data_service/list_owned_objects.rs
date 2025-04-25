// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_move_build::BuildConfig;
use sui_rpc_api::proto::rpc::v2alpha::live_data_service_client::LiveDataServiceClient;
use sui_rpc_api::proto::rpc::v2alpha::ListOwnedObjectsRequest;
use sui_rpc_api::proto::rpc::v2beta::changed_object::{IdOperation, OutputObjectState};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg, TransactionData, TransactionKind};
use sui_types::Identifier;
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_indexing_with_tto() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut channel = tonic::transport::Channel::from_shared(cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();

    let mut client = LiveDataServiceClient::new(channel.clone());
    let address = cluster.get_address_0();

    let objects = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(address.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    let gas = &objects[0];
    let coin = &objects[1];

    //
    // Publish the TTO package
    //

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "tto"]);
    let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
    let compiled_modules_bytes =
        compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(compiled_modules_bytes, dependencies);
    let ptb = builder.finish();
    let gas_data = sui_types::transaction::GasData {
        payment: vec![(
            gas.object_id().parse().unwrap(),
            gas.version().into(),
            gas.digest().parse().unwrap(),
        )],
        owner: address,
        price: 1000,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);

    let txn = cluster.wallet.sign_transaction(&tx_data);

    let transaction = crate::execute_transaction(&mut channel, &txn).await;

    //
    // Run the `start` function to initialize the setup and TTO a coin
    //

    let effects = transaction.effects.unwrap();
    let package_id = effects
        .changed_objects
        .iter()
        .find_map(|o| {
            if matches!(o.output_state(), OutputObjectState::PackageWrite) {
                Some(o.object_id().to_owned())
            } else {
                None
            }
        })
        .unwrap();
    let gas = effects.gas_object.unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id.parse().unwrap(),
            Identifier::new("M1").unwrap(),
            Identifier::new("start").unwrap(),
            vec![],
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
                coin.object_id().parse().unwrap(),
                coin.version().into(),
                coin.digest().parse().unwrap(),
            )))],
        )
        .unwrap();
    let ptb = builder.finish();

    let gas_data = sui_types::transaction::GasData {
        payment: vec![(
            gas.object_id().parse().unwrap(),
            gas.output_version().into(),
            gas.output_digest().parse().unwrap(),
        )],
        owner: address,
        price: 1000,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);

    let txn = cluster.wallet.sign_transaction(&tx_data);

    let transaction = crate::execute_transaction(&mut channel, &txn).await;

    let effects = transaction.effects.unwrap();
    let parent = effects
        .changed_objects
        .iter()
        .find_map(|o| {
            if matches!(o.id_operation(), IdOperation::Created) {
                Some((
                    o.object_id().to_owned(),
                    o.output_version(),
                    o.output_digest().to_owned(),
                ))
            } else {
                None
            }
        })
        .unwrap();
    let gas = effects.gas_object.unwrap();

    let coin = effects
        .changed_objects
        .iter()
        .find_map(|o| {
            if matches!(o.output_state(), OutputObjectState::ObjectWrite)
                && o.object_id() == coin.object_id()
            {
                Some((
                    o.object_id().to_owned(),
                    o.output_version(),
                    o.output_digest().to_owned(),
                ))
            } else {
                None
            }
        })
        .unwrap();

    // Parent starts with 1 coin
    assert_eq!(
        client
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some(parent.0.clone()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .objects
            .len(),
        1
    );

    // 0x0 starts with 0 coins
    assert!(client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some("0x0".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects
        .is_empty());

    //
    // Run the `receive` function to receive the coin from TTO and send it to 0x0
    //

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id.parse().unwrap(),
            Identifier::new("M1").unwrap(),
            Identifier::new("receive").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject((
                    parent.0.parse().unwrap(),
                    parent.1.into(),
                    parent.2.parse().unwrap(),
                ))),
                CallArg::Object(ObjectArg::Receiving((
                    coin.0.parse().unwrap(),
                    coin.1.into(),
                    coin.2.parse().unwrap(),
                ))),
            ],
        )
        .unwrap();
    let ptb = builder.finish();

    let gas_data = sui_types::transaction::GasData {
        payment: vec![(
            gas.object_id().parse().unwrap(),
            gas.output_version().into(),
            gas.output_digest().parse().unwrap(),
        )],
        owner: address,
        price: 1000,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);

    let txn = cluster.wallet.sign_transaction(&tx_data);

    crate::execute_transaction(&mut channel, &txn).await;

    // Parent ends with 0 coins
    assert!(client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(parent.0.clone()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects
        .is_empty());

    // 0x0 ends with 1 coin
    assert_eq!(
        client
            .list_owned_objects(ListOwnedObjectsRequest {
                owner: Some("0x0".to_owned()),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .objects
            .len(),
        1
    );
}
