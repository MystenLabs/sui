// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::{Checkpoint, ExecutedTransaction, GetCheckpointRequest, Object};
use test_cluster::TestClusterBuilder;

use crate::{stake_with_validator, transfer_coin};

#[sim_test]
async fn get_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;
    let transaction_digest = stake_with_validator(&test_cluster).await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request with no provided read_mask
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

    // Request all fields
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

    // Request by digest
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

    // Request by sequence_number
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

    // A Checkpoint that we know has a transaction that emitted an event
    let checkpoint = client
        .get_transaction(
            GetTransactionRequest::new(&transaction_digest)
                .with_read_mask(FieldMask::from_paths(["checkpoint"])),
        )
        .await
        .unwrap()
        .into_inner()
        .transaction
        .unwrap()
        .checkpoint
        .unwrap();

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
        if digest == Some(transaction_digest.to_string()) {
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
    // Ensure we found the transaction we used for picking the checkpoint to test against
    assert!(found_transaction);

    // Request all fields
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
                FieldMask::from_paths([
                    "sequence_number",
                    "digest",
                    "summary",
                    "signature",
                    "contents",
                    "transactions",
                    "objects",
                ]),
            ),
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

    let mut found_transaction = false;
    for ExecutedTransaction {
        digest,
        transaction,
        effects,
        events,
        objects,
        ..
    } in transactions
    {
        assert!(digest.is_some());
        if digest == Some(transaction_digest.to_string()) {
            found_transaction = true;
            assert!(events.is_some());
        }
        assert!(transaction.is_some());
        assert!(effects.is_some());
        assert!(objects.is_none()); // This doesn't get populated by this API
    }

    for Object {
        bcs,
        object_id,
        version,
        digest,
        owner,
        object_type,
        ..
    } in objects.unwrap().objects
    {
        assert!(object_id.is_some());
        assert!(version.is_some());
        assert!(digest.is_some());
        assert!(bcs.is_some());
        assert!(owner.is_some());
        assert!(object_type.is_some());
    }

    // Ensure we found the transaction we used for picking the checkpoint to test against
    assert!(found_transaction);
}
