// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors `sui-e2e-tests/tests/rpc/v2/ledger_service/get_checkpoint.rs`,
//! but adapted to the in-process Simulacrum harness: instead of
//! `transfer_coin` / `stake_with_validator` (which both submit
//! through the test-cluster wallet), we execute a simple Sui
//! transfer through Simulacrum directly and assert on what
//! comes back through the rpc-api.

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::Digest;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn get_checkpoint() {
    let cluster = LocalCluster::new().await.unwrap();

    // Build and submit a single Sui transfer through Simulacrum,
    // then close out a checkpoint so the indexer surfaces it.
    // The faucet amount has to cover at least one full gas
    // budget (~5 SUI = 5_000_000_000 MIST) plus the actual
    // transfer; over-provision to keep the test stable.
    let (sender, keypair, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    let rgp = cluster.reference_gas_price().await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(1), sender)
        .build();
    let signed = to_sender_signed_transaction(tx_data, &keypair);
    let tx_digest: Digest = (*signed.digest()).into();
    let tx_digest_str = tx_digest.to_string();
    let (_effects, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none(), "transfer should succeed: {err:?}");
    let new_checkpoint = cluster.create_checkpoint().await.unwrap();

    let mut client: LedgerServiceClient<Channel> =
        LedgerServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    // ---- Request with no provided read_mask ----
    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
        objects,
        ..
    } = client
        .get_checkpoint(GetCheckpointRequest::default())
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(transactions.is_empty());
    assert!(objects.is_none());

    // ---- Request all fields ----
    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
        objects,
        ..
    } = client
        .get_checkpoint(
            GetCheckpointRequest::latest().with_read_mask(FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary",
                "signature",
                "contents",
                "transactions",
                "objects",
            ])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(signature.is_some());
    assert!(contents.is_some());
    assert!(!transactions.is_empty());
    assert!(objects.is_some());

    // ---- Request by digest ----
    let response = client
        .get_checkpoint({
            let mut message = GetCheckpointRequest::default();
            message.checkpoint_id = Some(CheckpointId::Digest(digest.clone().unwrap()));
            message
        })
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();
    assert_eq!(response.digest, digest.to_owned());

    // ---- Request by sequence_number ----
    let response = client
        .get_checkpoint(GetCheckpointRequest::by_sequence_number(
            sequence_number.unwrap(),
        ))
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();
    assert_eq!(response.sequence_number, sequence_number.to_owned());
    assert_eq!(response.digest, digest.to_owned());

    // Pick a specific checkpoint via the transaction we
    // submitted — confirm `GetTransaction(checkpoint)` round-
    // trips the right sequence number.
    let checkpoint = client
        .get_transaction(
            GetTransactionRequest::new(&tx_digest)
                .with_read_mask(FieldMask::from_paths(["checkpoint"])),
        )
        // Annotate so the compiler binds the `digest`-side
        // parameter to `sui_sdk_types::Digest`.
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap()
        .checkpoint
        .unwrap();
    assert_eq!(checkpoint, new_checkpoint.sequence_number);

    // ---- Request the checkpoint by sequence, with just
    // sequence_number / digest / transactions.digest in the
    // read_mask. The transactions list should be populated but
    // every sub-field beyond digest should be `None` / empty. ----
    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
        objects,
        ..
    } = client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths(["sequence_number", "digest", "transactions.digest"]),
            ),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(objects.is_none());

    let mut found_transaction = false;
    for ExecutedTransaction {
        digest,
        transaction,
        effects,
        events,
        objects,
        signatures,
        checkpoint,
        timestamp,
        balance_changes,
        ..
    } in transactions
    {
        assert!(digest.is_some());
        if digest == Some(tx_digest_str.clone()) {
            found_transaction = true;
        }
        assert!(transaction.is_none());
        assert!(effects.is_none());
        assert!(events.is_none());
        assert!(objects.is_none());
        assert!(signatures.is_empty());
        assert!(checkpoint.is_none());
        assert!(timestamp.is_none());
        assert!(balance_changes.is_empty());
    }
    assert!(
        found_transaction,
        "tx submitted through Simulacrum should appear in its checkpoint",
    );
}
