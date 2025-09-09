// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use prost_types::FieldMask;
use sui_rpc::client::Client;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::{
    Bcs, ExecuteTransactionRequest, ExecutedTransaction, Transaction, UserSignature,
};

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
    assert!(transaction
        .effects
        .as_ref()
        .unwrap()
        .status
        .as_ref()
        .unwrap()
        .success());

    transaction
}
