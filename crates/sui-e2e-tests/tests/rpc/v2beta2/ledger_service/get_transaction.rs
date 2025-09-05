// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::stake_with_validator;
use sui_macros::sim_test;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2beta2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::{ExecutedTransaction, GetTransactionRequest};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_transaction() {
    let test_cluster = TestClusterBuilder::new().build().await;

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
