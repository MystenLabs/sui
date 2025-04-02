// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_json_rpc_api::{
    CoinReadApiClient, IndexerApiClient, TransactionBuilderClient, WriteApiClient,
};
use sui_json_rpc_types::{
    ObjectChange, SuiObjectDataOptions, SuiObjectResponseQuery, SuiTransactionBlockResponseOptions,
    TransactionBlockBytes,
};
use sui_move_build::BuildConfig;
use sui_types::base_types::SuiAddress;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{CallArg, ObjectArg, TransactionData, TransactionKind};
use sui_types::Identifier;
use test_cluster::TestClusterBuilder;

#[tokio::test]
async fn test_indexing_with_tto() {
    let cluster = TestClusterBuilder::new().build().await;

    let http_client = cluster.rpc_client();
    let address = cluster.get_address_0();

    let objects = http_client
        .get_owned_objects(
            address,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::new()
                    .with_type()
                    .with_owner()
                    .with_previous_transaction(),
            )),
            None,
            None,
        )
        .await
        .unwrap()
        .data;

    let gas = objects[0].object().unwrap();
    let coin = objects[1].object().unwrap();

    //
    // Publish the TTO package
    //

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "data", "tto"]);
    let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
    let compiled_modules_bytes =
        compiled_package.get_package_base64(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let transaction_bytes: TransactionBlockBytes = http_client
        .publish(
            address,
            compiled_modules_bytes,
            dependencies,
            Some(gas.object_id),
            100_000_000.into(),
        )
        .await
        .unwrap();

    let tx = cluster
        .wallet
        .sign_transaction(&transaction_bytes.to_data().unwrap());
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionBlockResponseOptions::new()
                    .with_object_changes()
                    .with_events(),
            ),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();

    //
    // Run the `start` function to initialize the setup and TTO a coin
    //

    let object_changes = tx_response.object_changes.unwrap();
    let package_id = object_changes
        .iter()
        .find_map(|e| {
            if let ObjectChange::Published { package_id, .. } = e {
                Some(package_id)
            } else {
                None
            }
        })
        .unwrap();
    let gas = object_changes
        .iter()
        .find_map(|c| {
            if let ObjectChange::Mutated {
                object_id,
                version,
                digest,
                ..
            } = c
            {
                if object_id == &gas.object_id {
                    Some((*object_id, *version, *digest))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id.to_owned(),
            Identifier::new("M1").unwrap(),
            Identifier::new("start").unwrap(),
            vec![],
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
                coin.object_ref(),
            ))],
        )
        .unwrap();
    let ptb = builder.finish();

    let gas_data = sui_types::transaction::GasData {
        payment: vec![gas],
        owner: address,
        price: 1000,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);

    let tx = cluster.wallet.sign_transaction(&tx_data);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let tx_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_object_changes(),
            ),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();

    let object_changes = tx_response.object_changes.unwrap();

    let parent = object_changes
        .iter()
        .find_map(|e| {
            if let ObjectChange::Created {
                object_id,
                version,
                digest,
                ..
            } = e
            {
                Some((*object_id, *version, *digest))
            } else {
                None
            }
        })
        .unwrap();

    let gas = object_changes
        .iter()
        .find_map(|c| {
            if let ObjectChange::Mutated {
                object_id,
                version,
                digest,
                ..
            } = c
            {
                if object_id == &gas.0 {
                    Some((*object_id, *version, *digest))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();

    let coin = object_changes
        .iter()
        .find_map(|c| {
            if let ObjectChange::Mutated {
                object_id,
                version,
                digest,
                ..
            } = c
            {
                if object_id == &coin.object_id {
                    Some((*object_id, *version, *digest))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();

    let parent_start_balance = http_client
        .get_balance(parent.0.into(), None)
        .await
        .unwrap()
        .total_balance;
    assert_eq!(
        http_client
            .get_balance(SuiAddress::ZERO, None)
            .await
            .unwrap()
            .total_balance,
        0
    );

    //
    // Run the `receive` function to receive the coin from TTO and send it to 0x0
    //

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package_id.to_owned(),
            Identifier::new("M1").unwrap(),
            Identifier::new("receive").unwrap(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
                CallArg::Object(ObjectArg::Receiving(coin)),
            ],
        )
        .unwrap();
    let ptb = builder.finish();

    let gas_data = sui_types::transaction::GasData {
        payment: vec![gas],
        owner: address,
        price: 1000,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);

    let tx = cluster.wallet.sign_transaction(&tx_data);
    let (tx_bytes, signatures) = tx.to_tx_bytes_and_signatures();

    let _tx_response = http_client
        .execute_transaction_block(
            tx_bytes,
            signatures,
            Some(
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_object_changes(),
            ),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();

    assert_eq!(
        http_client
            .get_balance(parent.0.into(), None)
            .await
            .unwrap()
            .total_balance,
        0
    );
    assert_eq!(
        http_client
            .get_balance(SuiAddress::ZERO, None)
            .await
            .unwrap()
            .total_balance,
        parent_start_balance
    );
}
