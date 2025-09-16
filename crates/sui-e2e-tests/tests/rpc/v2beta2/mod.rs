// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use prost_types::FieldMask;
use std::path::PathBuf;
use sui_move_build::BuildConfig;
use sui_rpc::client::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::{
    Bcs, ExecuteTransactionRequest, ExecutedTransaction, Transaction, UserSignature,
};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{TransactionData, TransactionKind};

mod ledger_service;
mod live_data_service;
mod move_package_service;
mod signature_verification_service;
mod subscription_service;
mod transaction_execution_service;

async fn execute_transaction(
    client: &mut Client,
    signed_transaction: &sui_types::transaction::Transaction,
) -> ExecutedTransaction {
    let mut transaction = Transaction::default();
    transaction.bcs = Some(Bcs::serialize(signed_transaction.transaction_data()).unwrap());

    let signatures = signed_transaction
        .tx_signatures()
        .iter()
        .map(|s| {
            let mut message = UserSignature::default();
            message.bcs = Some({
                let mut message = Bcs::default();
                message.value = Some(s.as_ref().to_owned().into());
                message
            });
            message
        })
        .collect();

    let mut request = ExecuteTransactionRequest::default();
    request.transaction = Some(transaction);
    request.signatures = signatures;
    request.read_mask = Some(FieldMask::from_paths(["finality", "transaction"]));

    let transaction = client
        .execute_transaction_and_wait_for_checkpoint(request, Duration::from_secs(10))
        .await
        .unwrap();

    // Assert that the txn was successful
    let status = transaction
        .effects
        .as_ref()
        .unwrap()
        .status
        .as_ref()
        .unwrap();
    assert!(status.success());

    transaction
}

async fn publish_package(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
    path: PathBuf,
) -> (ObjectID, ExecutedTransaction) {
    let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
    let compiled_modules_bytes = compiled_package.get_package_bytes(false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(compiled_modules_bytes, dependencies);
    let ptb = builder.finish();
    let gas_data = sui_types::transaction::GasData {
        payment: vec![(gas_object.0, gas_object.1, gas_object.2)],
        owner: address,
        price: gas_price,
        budget: 100_000_000,
    };

    let kind = TransactionKind::ProgrammableTransaction(ptb);
    let tx_data = TransactionData::new_with_gas_data(kind, address, gas_data);
    let txn = test_cluster.wallet.sign_transaction(&tx_data).await;

    let mut client = Client::new(test_cluster.rpc_url().to_owned()).unwrap();
    let transaction = execute_transaction(&mut client, &txn).await;

    let package_id = transaction
        .effects
        .as_ref()
        .unwrap()
        .changed_objects
        .iter()
        .find_map(|o| {
            use sui_rpc::proto::sui::rpc::v2beta2::changed_object::OutputObjectState;
            if o.output_state == Some(OutputObjectState::PackageWrite as i32) {
                o.object_id.as_ref().map(|id| id.parse().unwrap())
            } else {
                None
            }
        })
        .unwrap();

    (package_id, transaction)
}
