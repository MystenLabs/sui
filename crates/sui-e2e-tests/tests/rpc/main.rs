// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use prost_types::FieldMask;
use sui_rpc_api::{
    field_mask::FieldMaskUtil,
    proto::rpc::v2beta::{
        ledger_service_client::LedgerServiceClient,
        transaction_execution_service_client::TransactionExecutionServiceClient, Bcs,
        ExecuteTransactionRequest, ExecuteTransactionResponse, ExecutedTransaction,
        GetTransactionRequest, Transaction, UserSignature,
    },
};

mod client;
mod ledger_service;
mod live_data_service;
mod subscription_service;
mod transaction_execution_service;

async fn transfer_coin(
    context: &sui_sdk::wallet_context::WalletContext,
) -> sui_sdk_types::TransactionDigest {
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let receiver = accounts_and_objs[1].0;
    let gas_object = accounts_and_objs[0].1[0];
    let object_to_send = accounts_and_objs[0].1[1];
    let txn = context.sign_transaction(
        &sui_test_transaction_builder::TestTransactionBuilder::new(sender, gas_object, gas_price)
            .transfer(object_to_send, receiver)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    resp.digest.into()
}

async fn stake_with_validator(
    cluster: &test_cluster::TestCluster,
) -> sui_sdk_types::TransactionDigest {
    let context = &cluster.wallet;
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let gas_object = accounts_and_objs[0].1[0];
    let coin_to_stake = accounts_and_objs[0].1[1];
    let validator_address = cluster.swarm.config().validator_configs()[0].sui_address();
    let txn = context.sign_transaction(
        &sui_test_transaction_builder::TestTransactionBuilder::new(sender, gas_object, gas_price)
            .call_staking(coin_to_stake, validator_address)
            .build(),
    );
    let resp = context.execute_transaction_must_succeed(txn).await;
    resp.digest.into()
}

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
                    read_mask: None,
                })
                .await
            {
                break poll_response;
            }
        }
    })
    .await
    .unwrap()
}
