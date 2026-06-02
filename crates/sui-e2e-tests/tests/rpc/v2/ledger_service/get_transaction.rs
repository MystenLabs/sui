// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::stake_with_validator;
use move_core_types::ident_str;
use sui_macros::sim_test;
use sui_rpc::Client;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::{ExecutedTransaction, GetTransactionRequest};
use sui_types::base_types::ObjectID;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_transaction() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let transaction_digest = stake_with_validator(&test_cluster).await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request with no provided read_mask
    let ExecutedTransaction {
        digest,
        transaction,
        signatures,
        effects,
        events,
        checkpoint,
        timestamp,
        ..
    } = client
        .get_transaction(GetTransactionRequest::new(&transaction_digest))
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    // These fields default to being read
    assert_eq!(digest, Some(transaction_digest.to_string()));

    // while these fields default to not being read
    assert!(transaction.is_none());
    assert!(signatures.is_empty());
    assert!(effects.is_none());
    assert!(events.is_none());
    assert!(checkpoint.is_none());
    assert!(timestamp.is_none());

    // Request all fields
    let ExecutedTransaction {
        digest,
        transaction,
        signatures,
        effects,
        events,
        checkpoint,
        timestamp,
        ..
    } = client
        .get_transaction(
            GetTransactionRequest::new(&transaction_digest).with_read_mask(FieldMask::from_paths(
                [
                    "digest",
                    "transaction",
                    "signatures",
                    "effects",
                    "events",
                    "checkpoint",
                    "timestamp",
                ],
            )),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert_eq!(digest, Some(transaction_digest.to_string()));
    assert!(transaction.is_some());
    assert!(!signatures.is_empty());
    assert!(effects.is_some());
    assert!(events.is_some());
    assert!(checkpoint.is_some());
    assert!(timestamp.is_some());
}

#[sim_test]
async fn get_transaction_exposes_execution_error_metadata() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let (sender, gas_object) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let gas_price = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            ObjectID::from_single_byte(1),
            ident_str!("option").to_owned(),
            ident_str!("bad_function").to_owned(),
            vec![],
            vec![],
        )
        .unwrap();
    let transaction_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        builder.finish(),
        50_000_000,
        gas_price,
    );
    let signed_transaction = test_cluster
        .wallet
        .sign_transaction(&transaction_data)
        .await;

    let mut execution_client = Client::new(test_cluster.rpc_url().to_owned()).unwrap();
    let executed_transaction =
        super::super::execute_transaction_assert_failed(&mut execution_client, &signed_transaction)
            .await;
    let transaction_digest: sui_sdk_types::Digest =
        executed_transaction.digest.unwrap().parse().unwrap();

    let mut ledger_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let metadata = ledger_client
        .get_transaction(
            GetTransactionRequest::new(&transaction_digest)
                .with_read_mask(FieldMask::from_paths(["effects.status.error.metadata"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap()
        .effects
        .unwrap()
        .status
        .unwrap()
        .error
        .unwrap()
        .metadata
        .unwrap();

    assert!(
        metadata
            .message
            .as_deref()
            .is_some_and(|message| message.contains("bad_function")),
        "unexpected execution error metadata: {:?}",
        metadata.message
    );
}
