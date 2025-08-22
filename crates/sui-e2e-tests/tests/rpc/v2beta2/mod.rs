// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use prost_types::FieldMask;
use std::path::PathBuf;
use sui_move_build::BuildConfig;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::{
    ledger_service_client::LedgerServiceClient,
    transaction_execution_service_client::TransactionExecutionServiceClient, Bcs,
    ExecuteTransactionRequest, ExecuteTransactionResponse, ExecutedTransaction,
    GetTransactionRequest, Transaction, UserSignature,
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
    channel: &mut tonic::transport::Channel,
    signed_transaction: &sui_types::transaction::Transaction,
) -> ExecutedTransaction {
    let mut client = TransactionExecutionServiceClient::new(&mut *channel);

    let ExecuteTransactionResponse {
        finality: _,
        transaction,
    } = client
        .execute_transaction(ExecuteTransactionRequest {
            transaction: Some(Transaction {
                bcs: Some(Bcs::serialize(signed_transaction.transaction_data()).unwrap()),
                ..Default::default()
            }),
            signatures: signed_transaction
                .tx_signatures()
                .iter()
                .map(|s| UserSignature {
                    bcs: Some(Bcs {
                        name: None,
                        value: Some(s.as_ref().to_owned().into()),
                    }),
                    ..Default::default()
                })
                .collect(),
            read_mask: Some(FieldMask::from_paths(["finality", "transaction"])),
        })
        .await
        .unwrap()
        .into_inner();

    let transaction = transaction.unwrap();

    // Assert that the txn was successful
    assert!(transaction
        .effects
        .as_ref()
        .unwrap()
        .status
        .as_ref()
        .unwrap()
        .success());

    wait_for_transaction(channel, transaction.digest()).await;

    transaction
}

async fn wait_for_transaction(
    channel: &mut tonic::transport::Channel,
    digest: &str,
) -> tonic::Response<ExecutedTransaction> {
    const WAIT_FOR_LOCAL_EXECUTION_TIMEOUT: Duration = Duration::from_secs(10);
    const WAIT_FOR_LOCAL_EXECUTION_DELAY: Duration = Duration::from_millis(200);
    const WAIT_FOR_LOCAL_EXECUTION_INTERVAL: Duration = Duration::from_millis(500);

    let mut client = LedgerServiceClient::new(channel);

    tokio::time::timeout(WAIT_FOR_LOCAL_EXECUTION_TIMEOUT, async {
        // Apply a short delay to give the full node a chance to catch up.
        tokio::time::sleep(WAIT_FOR_LOCAL_EXECUTION_DELAY).await;

        let mut interval = tokio::time::interval(WAIT_FOR_LOCAL_EXECUTION_INTERVAL);
        loop {
            interval.tick().await;

            if let Ok(poll_response) = client
                .get_transaction(GetTransactionRequest {
                    digest: Some(digest.to_owned()),
                    read_mask: Some(FieldMask::from_paths(["checkpoint"])),
                })
                .await
            {
                // Only break if the transaction has been included in a checkpoint
                if let Some(ref transaction) = poll_response.get_ref().transaction {
                    if transaction.checkpoint.is_some() {
                        break poll_response;
                    }
                }
            }
        }
    })
    .await
    .unwrap()
    .map(|response| response.transaction.unwrap())
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

    let mut channel = tonic::transport::Channel::from_shared(test_cluster.rpc_url().to_owned())
        .unwrap()
        .connect()
        .await
        .unwrap();
    let transaction = execute_transaction(&mut channel, &txn).await;

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
