// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::{
    ledger_service_client::LedgerServiceClient,
    transaction_execution_service_client::TransactionExecutionServiceClient, Bcs,
    ExecuteTransactionRequest, ExecuteTransactionResponse, ExecutedTransaction,
    GetTransactionRequest, Transaction, UserSignature,
};

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

    let mut transaction = Transaction::default();
    transaction.bcs = Some(Bcs::serialize(signed_transaction.transaction_data()).unwrap());
    let ExecuteTransactionResponse { transaction, .. } = client
        .execute_transaction(
            ExecuteTransactionRequest::new(transaction)
                .with_signatures(
                    signed_transaction
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
                        .collect(),
                )
                .with_read_mask(FieldMask::from_paths(["finality", "transaction"])),
        )
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
                .get_transaction({
                    let mut request = GetTransactionRequest::default();
                    request.digest = Some(digest.to_owned());
                    request
                })
                .await
            {
                break poll_response;
            }
        }
    })
    .await
    .unwrap()
    .map(|response| response.transaction.unwrap())
}
