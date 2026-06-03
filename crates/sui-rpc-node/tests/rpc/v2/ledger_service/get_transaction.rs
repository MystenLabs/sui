// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors `sui-e2e-tests/tests/rpc/v2/ledger_service/get_transaction.rs`.
//! Submits a transfer through Simulacrum, then asserts that the
//! `GetTransaction` field-mask defaults and explicit projections
//! line up with what `sui-rpc-api` returns.

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::Digest;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn get_transaction() {
    let cluster = LocalCluster::new().await.unwrap();
    let (sender, keypair, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    let rgp = cluster.reference_gas_price().await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(1), sender)
        .build();
    let signed = to_sender_signed_transaction(tx_data, &keypair);
    let tx_digest: Digest = (*signed.digest()).into();
    let (_effects, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none(), "transfer should succeed: {err:?}");
    cluster.create_checkpoint().await.unwrap();

    let mut client: LedgerServiceClient<Channel> =
        LedgerServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    // Request with no provided read_mask — digest only.
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
        .get_transaction(GetTransactionRequest::new(&tx_digest))
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert_eq!(digest, Some(tx_digest.to_string()));
    assert!(transaction.is_none());
    assert!(signatures.is_empty());
    assert!(effects.is_none());
    assert!(events.is_none());
    assert!(checkpoint.is_none());
    assert!(timestamp.is_none());

    // Request all fields.
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
        .get_transaction(GetTransactionRequest::new(&tx_digest).with_read_mask(
            FieldMask::from_paths([
                "digest",
                "transaction",
                "signatures",
                "effects",
                "events",
                "checkpoint",
                "timestamp",
            ]),
        ))
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap();

    assert_eq!(digest, Some(tx_digest.to_string()));
    assert!(transaction.is_some());
    assert!(!signatures.is_empty());
    assert!(effects.is_some());
    // `events` is only populated for transactions that emitted
    // events; the original e2e test exercises a `stake_with_validator`
    // call that does emit. A bare `transfer_sui` doesn't, so the
    // field is left absent — we just don't assert on it here.
    let _ = events;
    assert!(checkpoint.is_some());
    assert!(timestamp.is_some());
}
