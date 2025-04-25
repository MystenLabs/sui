// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_move_build::BuildConfig;
use sui_rpc_api::proto::rpc::v2alpha::live_data_service_client::LiveDataServiceClient;
use sui_rpc_api::proto::rpc::v2alpha::{
    GetCoinInfoRequest, GetCoinInfoResponse, ListOwnedObjectsRequest,
};
use sui_rpc_api::proto::rpc::v2beta::changed_object::{IdOperation, OutputObjectState};
use sui_sdk_types::TypeTag;
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

#[tokio::test]
async fn test_filter_by_type() {
    let cluster = TestClusterBuilder::new().build().await;

    let sui = "0x2::coin::Coin<0x2::sui::SUI>"
        .parse::<TypeTag>()
        .unwrap()
        .to_string();
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

    // We start with some SUI coins
    assert!(!objects.is_empty());
    assert!(objects.iter().all(|o| o.object_type() == sui));

    let gas = &objects[0];

    //
    // Publish the coin package
    //

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "rpc", "data", "trusted_coin"]);
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

    let effects = transaction.effects.unwrap();
    let gas = effects.gas_object.unwrap();
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

    let trusted = format!("{package_id}::trusted_coin::TRUSTED_COIN")
        .parse::<TypeTag>()
        .unwrap()
        .to_string();
    let trusted_coin = format!("0x2::coin::Coin<{trusted}>")
        .parse::<TypeTag>()
        .unwrap()
        .to_string();
    let treasury_cap_type =
        sui_types::coin::TreasuryCap::type_(sui_types::parse_sui_struct_tag(&trusted).unwrap())
            .to_canonical_string(true);

    let treasury_cap = effects
        .changed_objects
        .iter()
        .find_map(|o| {
            if matches!(o.id_operation(), IdOperation::Created)
                && treasury_cap_type == o.object_type()
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

    // After publishing we have the treasury cap and the coin meta data has 0 supply
    let GetCoinInfoResponse {
        coin_type,
        metadata,
        treasury,
        ..
    } = client
        .get_coin_info(GetCoinInfoRequest {
            coin_type: Some(trusted.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    let metadata = metadata.unwrap();
    assert_eq!(coin_type.as_ref(), Some(&trusted));
    assert_eq!(metadata.symbol(), "TRUSTED");
    assert_eq!(metadata.description(), "Trusted Coin for test");
    assert_eq!(metadata.name(), "Trusted Coin");
    assert_eq!(metadata.decimals(), 2);
    assert_eq!(treasury.unwrap().total_supply, Some(0));

    let objects = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(address.to_string()),
            object_type: Some(treasury_cap_type.clone()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].object_type(), treasury_cap_type);

    //
    // Mint some coins
    //
    let mut builder = ProgrammableTransactionBuilder::new();

    builder
        .move_call(
            package_id.parse().unwrap(),
            Identifier::new("trusted_coin").unwrap(),
            Identifier::new("mint").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject((
                    treasury_cap.0.parse().unwrap(),
                    treasury_cap.1.into(),
                    treasury_cap.2.parse().unwrap(),
                ))),
                CallArg::from(100_000u64),
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

    // After minting we should have some of the new coins and the supply should have updated
    let GetCoinInfoResponse {
        coin_type,
        treasury,
        ..
    } = client
        .get_coin_info(GetCoinInfoRequest {
            coin_type: Some(trusted.clone()),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(coin_type.as_ref(), Some(&trusted));
    assert_eq!(treasury.unwrap().total_supply, Some(100_000));

    let objects = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(address.to_string()),
            object_type: Some(trusted_coin.clone()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].object_type(), trusted_coin);

    // Calling `list_owned_objects` with `0x2::coin::Coin` filter (without a type T) should return
    // all coins
    let objects = client
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(address.to_string()),
            object_type: Some("0x2::coin::Coin".to_owned()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .objects;

    assert_eq!(objects.len(), 6);
    assert_eq!(
        objects
            .iter()
            .filter(|o| o.object_type() == trusted_coin)
            .count(),
        1
    );
    assert_eq!(objects.iter().filter(|o| o.object_type() == sui).count(), 5);
}
