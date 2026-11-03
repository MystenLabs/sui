// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

use prost::Message;
use prost_types::FieldMask;
use sui_move_build::BuildConfig;
use sui_rpc::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::ErrorInfo;
use sui_rpc::proto::google::rpc::Status as RpcStatus;
use sui_rpc::proto::sui::rpc::v2::ErrorReason;
use sui_rpc::proto::sui::rpc::v2::{
    Bcs, ExecuteTransactionRequest, ExecutedTransaction, Transaction, UserSignature,
};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{TransactionData, TransactionKind};

mod ledger_service;
mod move_package_service;
mod signature_verification_service;
mod state_service;
mod subscription_service;
mod transaction_execution_service;
mod unchanged_loaded_runtime_objects;

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
    request.read_mask = Some(FieldMask::from_paths(["*"]));

    let transaction = client
        .execute_transaction_and_wait_for_checkpoint(request, Duration::from_secs(10))
        .await
        .unwrap()
        .into_inner()
        .transaction()
        .to_owned();

    // Assert that the txn was successful
    assert!(
        transaction
            .effects
            .as_ref()
            .unwrap()
            .status
            .as_ref()
            .unwrap()
            .success()
    );

    transaction
}

async fn execute_transaction_assert_failed(
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
    request.read_mask = Some(FieldMask::from_paths(["*"]));

    let transaction = client
        .execute_transaction_and_wait_for_checkpoint(request, Duration::from_secs(10))
        .await
        .unwrap()
        .into_inner()
        .transaction()
        .to_owned();

    // Assert that the txn was successful
    assert!(!transaction.effects().status().success());

    transaction
}

async fn publish_package(
    test_cluster: &test_cluster::TestCluster,
    address: SuiAddress,
    path: PathBuf,
) -> (ObjectID, ExecutedTransaction) {
    let compiled_package = BuildConfig::new_for_testing()
        .build_async(&path)
        .await
        .unwrap();
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
            use sui_rpc::proto::sui::rpc::v2::changed_object::OutputObjectState;
            if o.output_state == Some(OutputObjectState::PackageWrite as i32) {
                o.object_id
                    .as_ref()
                    .and_then(|id| ObjectID::from_str(id).ok())
            } else {
                None
            }
        })
        .unwrap();

    (package_id, transaction)
}

/// A failed call's `google.rpc.Status`: a `tonic::Status` carries it encoded in its details, and a
/// per-item batch error is one.
trait RpcStatusExt {
    fn rpc_status(&self) -> RpcStatus;
}

impl RpcStatusExt for tonic::Status {
    fn rpc_status(&self) -> RpcStatus {
        RpcStatus::decode(self.details()).unwrap()
    }
}

impl RpcStatusExt for RpcStatus {
    fn rpc_status(&self) -> RpcStatus {
        self.clone()
    }
}

/// The `google.rpc.ErrorInfo` among `status`'s details.
fn error_info(status: &RpcStatus) -> ErrorInfo {
    status
        .details
        .iter()
        .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        .unwrap_or_else(|| panic!("no ErrorInfo in {status:?}"))
}

/// Asserts `status` is `OutOfRange` with a `BELOW_EARLIEST_AVAILABLE` ErrorInfo naming
/// `floor_checkpoint`.
fn assert_below_earliest_available(status: &impl RpcStatusExt, floor_checkpoint: u64) {
    let status = status.rpc_status();
    assert_eq!(status.code, tonic::Code::OutOfRange as i32, "{status:?}");
    let error_info = error_info(&status);
    assert_eq!(
        error_info.reason,
        ErrorReason::BelowEarliestAvailable.as_str_name()
    );
    assert_eq!(error_info.domain, ErrorReason::DOMAIN);
    assert_eq!(
        error_info.metadata,
        BTreeMap::from([(
            "lowestAvailableCheckpoint".to_owned(),
            floor_checkpoint.to_string()
        )])
    );
}

/// Asserts `status` is `OutOfRange` with an `ABOVE_LATEST_AVAILABLE` ErrorInfo for `bound` at
/// `requested_checkpoint`, whose first unavailable checkpoint lies at or below the requested one.
fn assert_above_latest_available(
    status: &impl RpcStatusExt,
    bound: &str,
    requested_checkpoint: u64,
) {
    let status = status.rpc_status();
    assert_eq!(status.code, tonic::Code::OutOfRange as i32, "{status:?}");
    let error_info = error_info(&status);
    assert_eq!(
        error_info.reason,
        ErrorReason::AboveLatestAvailable.as_str_name()
    );
    assert_eq!(error_info.domain, ErrorReason::DOMAIN);
    let metadata = &error_info.metadata;
    assert_eq!(metadata.len(), 3, "{metadata:?}");
    assert_eq!(metadata["bound"], bound);
    assert_eq!(
        metadata["requestedCheckpoint"],
        requested_checkpoint.to_string()
    );
    let first_unavailable: u64 = metadata["firstUnavailableCheckpoint"].parse().unwrap();
    assert!(first_unavailable <= requested_checkpoint, "{metadata:?}");
}
