// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_rpc::client::v2::Client;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::changed_object::IdOperation;
use sui_rpc::proto::sui::rpc::v2::changed_object::OutputObjectState;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetObjectRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::CallArg;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::SharedObjectMutability;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TransactionKind;
use sui_types::Identifier;
use test_cluster::TestClusterBuilder;

use crate::{stake_with_validator, transfer_coin};

#[sim_test]
async fn test_unchanged_loaded_runtime_objects() {
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;
    let transaction_digest = stake_with_validator(&test_cluster).await;

    let mut client = sui_rpc::client::v2::Client::new(test_cluster.rpc_url()).unwrap();
    let t = client
        .ledger_client()
        .get_transaction(
            GetTransactionRequest::new(&transaction_digest)
                .with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert!(t.effects().unchanged_loaded_runtime_objects().is_empty());

    let c = client
        .ledger_client()
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(t.checkpoint())
                .with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(!c
        .objects()
        .objects()
        .iter()
        .any(|o| o.object_type() == "package"));

    let address = test_cluster.get_address_0();
    let objects = client
        .state_client()
        .list_owned_objects(
            sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest::default()
                .with_owner(address.to_string())
                .with_read_mask(FieldMask::from_str("object_id,version,digest,object_type")),
        )
        .await
        .unwrap()
        .into_inner()
        .objects;

    let gas = &objects[0];

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            "0x3".parse().unwrap(),
            Identifier::new("sui_system").unwrap(),
            Identifier::new("active_validator_voting_powers").unwrap(),
            vec![],
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: "0x5".parse().unwrap(),
                initial_shared_version: 1.into(),
                mutability: SharedObjectMutability::Immutable,
            })],
        )
        .unwrap();
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

    let txn = test_cluster.wallet.sign_transaction(&tx_data).await;
    let transaction_digest = (*txn.digest()).into();

    // TODO: The data that we get back from execute_transaction isn't complete since we still need
    // to pipe through the info from the validators
    let _transaction = super::execute_transaction(&mut client, &txn).await;

    let t = client
        .ledger_client()
        .get_transaction(
            GetTransactionRequest::new(&transaction_digest)
                .with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert_eq!(t.effects().unchanged_loaded_runtime_objects().len(), 1);
    assert_eq!(
        t.effects().unchanged_loaded_runtime_objects()[0].object_id(),
        "0x5b890eaf2abcfa2ab90b77b8e6f3d5d8609586c3e583baf3dccd5af17edf48d1"
    );

    assert_eq!(t.effects().unchanged_consensus_objects().len(), 1);
    assert_eq!(
        t.effects().unchanged_consensus_objects()[0].object_id(),
        "0x0000000000000000000000000000000000000000000000000000000000000005"
    );
}

#[sim_test]
async fn test_tto_receive_twice() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut client = Client::new(cluster.rpc_url().to_owned()).unwrap();
    let address = cluster.get_address_0();

    let objects = client
        .state_client()
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some(address.to_string());
            message.read_mask = Some(FieldMask::from_str("object_id,version,digest,object_type"));
            message
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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    let transaction = super::execute_transaction(&mut client, &txn).await;

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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    let transaction = super::execute_transaction(&mut client, &txn).await;

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
            .state_client()
            .list_owned_objects({
                let mut message = ListOwnedObjectsRequest::default();
                message.owner = Some(parent.0.clone());
                message
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
        .state_client()
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some("0x0".to_owned());
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects
        .is_empty());

    //
    // Run the `receive` function to receive the coin from TTO twice, which will result in a fail
    // but the recieved object should have been read during execution and been unchanged.
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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    super::execute_transaction_assert_failed(&mut client, &txn).await;

    let t = client
        .ledger_client()
        .get_transaction(
            sui_rpc::proto::sui::rpc::v2::GetTransactionRequest::new(&(*txn.digest()).into())
                .with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert_eq!(t.effects().unchanged_loaded_runtime_objects().len(), 1);
    assert_eq!(
        t.effects().unchanged_loaded_runtime_objects()[0].object_id(),
        coin.0.as_str(),
    );
    // received object does not show up in changed obejcts
    assert!(!t
        .effects()
        .changed_objects()
        .iter()
        .any(|o| o.object_id() == coin.0.as_str()));
}

#[sim_test]
async fn test_tto_success() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut client = Client::new(cluster.rpc_url().to_owned()).unwrap();
    let address = cluster.get_address_0();

    let objects = client
        .state_client()
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some(address.to_string());
            message.read_mask = Some(FieldMask::from_str("object_id,version,digest,object_type"));
            message
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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    let transaction = super::execute_transaction(&mut client, &txn).await;

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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    let transaction = super::execute_transaction(&mut client, &txn).await;

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
            .state_client()
            .list_owned_objects({
                let mut message = ListOwnedObjectsRequest::default();
                message.owner = Some(parent.0.clone());
                message
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
        .state_client()
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some("0x0".to_owned());
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects
        .is_empty());

    //
    // Run the `receive` function to receive the coin from TTO twice, which will result in a fail
    // but the recieved object should have been read during execution and been unchanged.
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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    super::execute_transaction(&mut client, &txn).await;

    let t = client
        .ledger_client()
        .get_transaction(
            sui_rpc::proto::sui::rpc::v2::GetTransactionRequest::new(&(*txn.digest()).into())
                .with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    // No unchanged_loaded_runtime_objects
    assert!(t.effects().unchanged_loaded_runtime_objects().is_empty());
    // received object shows up in changed obejcts
    assert!(t
        .effects()
        .changed_objects()
        .iter()
        .any(|o| o.object_id() == coin.0.as_str()));

    // Parent ends with 0 coins
    assert!(client
        .state_client()
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some(parent.0.clone());
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects
        .is_empty());

    // 0x0 ends with 1 coin
    assert_eq!(
        client
            .state_client()
            .list_owned_objects({
                let mut message = ListOwnedObjectsRequest::default();
                message.owner = Some("0x0".to_owned());
                message
            })
            .await
            .unwrap()
            .into_inner()
            .objects
            .len(),
        1
    );

    //
    // Try to receive but fail
    //
    let effects = t.effects.unwrap();
    let gas = effects.gas_object.unwrap();

    let parent = client
        .ledger_client()
        .get_object(GetObjectRequest::default().with_object_id(parent.0))
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    let coin = client
        .ledger_client()
        .get_object(GetObjectRequest::default().with_object_id(coin.0))
        .await
        .unwrap()
        .into_inner()
        .object
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id.parse().unwrap(),
            Identifier::new("M1").unwrap(),
            Identifier::new("receive").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject((
                    parent.object_id().parse().unwrap(),
                    parent.version().into(),
                    parent.digest().parse().unwrap(),
                ))),
                CallArg::Object(ObjectArg::Receiving((
                    coin.object_id().parse().unwrap(),
                    coin.version().into(),
                    coin.digest().parse().unwrap(),
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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    super::execute_transaction_assert_failed(&mut client, &txn).await;

    let t = client
        .ledger_client()
        .get_transaction(
            sui_rpc::proto::sui::rpc::v2::GetTransactionRequest::new(&(*txn.digest()).into())
                .with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    // No unchanged_loaded_runtime_objects
    assert!(t.effects().unchanged_loaded_runtime_objects().is_empty());
}

#[sim_test]
async fn test_receive_input() {
    let cluster = TestClusterBuilder::new().build().await;

    let mut client = Client::new(cluster.rpc_url().to_owned()).unwrap();
    let address = cluster.get_address_0();

    let objects = client
        .state_client()
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some(address.to_string());
            message.read_mask = Some(FieldMask::from_str("object_id,version,digest,object_type"));
            message
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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    let transaction = super::execute_transaction(&mut client, &txn).await;

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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    let transaction = super::execute_transaction(&mut client, &txn).await;

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
            .state_client()
            .list_owned_objects({
                let mut message = ListOwnedObjectsRequest::default();
                message.owner = Some(parent.0.clone());
                message
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
        .state_client()
        .list_owned_objects({
            let mut message = ListOwnedObjectsRequest::default();
            message.owner = Some("0x0".to_owned());
            message
        })
        .await
        .unwrap()
        .into_inner()
        .objects
        .is_empty());

    //
    // Run the `receive` function to receive the coin from TTO twice, which will result in a fail
    // but the recieved object should have been read during execution and been unchanged.
    //

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id.parse().unwrap(),
            Identifier::new("M1").unwrap(),
            Identifier::new("dont_receive").unwrap(),
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

    let txn = cluster.wallet.sign_transaction(&tx_data).await;

    super::execute_transaction(&mut client, &txn).await;

    let t = client
        .ledger_client()
        .get_transaction(
            sui_rpc::proto::sui::rpc::v2::GetTransactionRequest::new(&(*txn.digest()).into())
                .with_read_mask(FieldMask::from_paths(["*"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert!(t.effects().unchanged_loaded_runtime_objects().is_empty());
    // received object does not show up in changed obejcts
    assert!(!t
        .effects()
        .changed_objects()
        .iter()
        .any(|o| o.object_id() == coin.0.as_str()));
}
