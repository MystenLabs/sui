// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::stake_with_validator;
use sui_macros::sim_test;
use sui_rpc_api::field_mask::FieldMask;
use sui_rpc_api::field_mask::FieldMaskUtil;
use sui_rpc_api::proto::node::v2::node_service_client::NodeServiceClient;
use sui_rpc_api::proto::node::v2::{GetTransactionRequest, GetTransactionResponse};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_transaction() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let transaction_digest = stake_with_validator(&test_cluster).await;

    let mut grpc_client = NodeServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request with no provided read_mask
    let GetTransactionResponse {
        digest,
        transaction,
        transaction_bcs,
        signatures,
        signatures_bytes,
        effects,
        effects_bcs,
        events,
        events_bcs,
        checkpoint,
        timestamp,
    } = grpc_client
        .get_transaction(GetTransactionRequest::new(transaction_digest))
        .await
        .unwrap()
        .into_inner();

    // These fields default to being read
    assert!(digest.is_some());

    // while these fields default to not being read
    assert!(transaction.is_none());
    assert!(transaction_bcs.is_none());
    assert!(signatures.is_empty());
    assert!(signatures_bytes.is_empty());
    assert!(effects.is_none());
    assert!(effects_bcs.is_none());
    assert!(events.is_none());
    assert!(events_bcs.is_none());
    assert!(checkpoint.is_none());
    assert!(timestamp.is_none());

    // Request all fields
    let response = grpc_client
        .get_transaction(
            GetTransactionRequest::new(transaction_digest).with_read_mask(FieldMask::from_paths([
                "digest",
                "transaction",
                "transaction_bcs",
                "signatures",
                "signatures_bytes",
                "effects",
                "effects_bcs",
                "events",
                "events_bcs",
                "checkpoint",
                "timestamp",
            ])),
        )
        .await
        .unwrap()
        .into_inner();

    let GetTransactionResponse {
        digest,
        transaction,
        transaction_bcs,
        signatures,
        signatures_bytes,
        effects,
        effects_bcs,
        events,
        events_bcs,
        checkpoint,
        timestamp,
    } = &response;

    assert!(digest.is_some());
    assert!(transaction.is_some());
    assert!(transaction_bcs.is_some());
    assert!(!signatures.is_empty());
    assert!(!signatures_bytes.is_empty());
    assert!(effects.is_some());
    assert!(effects_bcs.is_some());
    assert!(events.is_some());
    assert!(events_bcs.is_some());
    assert!(checkpoint.is_some());
    assert!(timestamp.is_some());
}
