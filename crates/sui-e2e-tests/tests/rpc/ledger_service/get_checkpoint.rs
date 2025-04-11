// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::field_mask::FieldMask;
use sui_rpc_api::field_mask::FieldMaskUtil;
use sui_rpc_api::proto::rpc::v2beta::get_checkpoint_request::CheckpointId;
use sui_rpc_api::proto::rpc::v2beta::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::rpc::v2beta::GetTransactionRequest;
use sui_rpc_api::proto::rpc::v2beta::{
    Checkpoint, ExecutedTransaction, GetCheckpointRequest, Object,
};
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
    } = client
        .get_checkpoint(GetCheckpointRequest::default())
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(transactions.is_empty());

    // Request all fields
    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
    } = client
        .get_checkpoint(GetCheckpointRequest {
            checkpoint_id: None,
            read_mask: Some(FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary",
                "signature",
                "contents",
                "transactions",
            ])),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(signature.is_some());
    assert!(contents.is_some());
    assert!(!transactions.is_empty());

    // Request by digest
    let response = client
        .get_checkpoint(GetCheckpointRequest {
            checkpoint_id: Some(CheckpointId::Digest(digest.clone().unwrap())),
            read_mask: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.digest, digest.to_owned());

    // Request by sequence_number
    let response = client
        .get_checkpoint(GetCheckpointRequest {
            checkpoint_id: Some(CheckpointId::SequenceNumber(sequence_number.unwrap())),
            read_mask: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.sequence_number, sequence_number.to_owned());
    assert_eq!(response.digest, digest.to_owned());

    // A Checkpoint that we know has a transaction that emitted an event
    let checkpoint = client
        .get_transaction(GetTransactionRequest {
            digest: Some(transaction_digest.to_string()),
            read_mask: Some(FieldMask::from_paths(["checkpoint"])),
        })
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    let Checkpoint {
        sequence_number,
        digest,
        summary,
        signature,
        contents,
        transactions,
    } = client
        .get_checkpoint(GetCheckpointRequest {
            checkpoint_id: Some(CheckpointId::SequenceNumber(checkpoint)),
            read_mask: Some(FieldMask::from_paths([
                "sequence_number",
                "digest",
                "transactions.digest",
            ])),
        })
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());

    let mut found_transaction = false;
    for ExecutedTransaction {
        digest,
        transaction,
        effects,
        events,
        input_objects,
        output_objects,
        signatures,
        checkpoint,
        timestamp,
        balance_changes,
    } in transactions
    {
        assert!(digest.is_some());
        if digest == Some(transaction_digest.to_string()) {
            found_transaction = true;
        }
        assert!(transaction.is_none());
        assert!(effects.is_none());
        assert!(events.is_none());
        assert!(input_objects.is_empty());
        assert!(output_objects.is_empty());
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
    } = client
        .get_checkpoint(GetCheckpointRequest {
            checkpoint_id: Some(CheckpointId::SequenceNumber(checkpoint)),
            read_mask: Some(FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary",
                "signature",
                "contents",
                "transactions",
            ])),
        })
        .await
        .unwrap()
        .into_inner();

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
        input_objects,
        output_objects,
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
        assert!(!input_objects.is_empty());
        assert!(!output_objects.is_empty());

        for Object {
            bcs,
            object_id,
            version,
            digest,
            owner,
            object_type,
            ..
        } in input_objects.iter().chain(output_objects.iter())
        {
            assert!(object_id.is_some());
            assert!(version.is_some());
            assert!(digest.is_some());
            assert!(bcs.is_some());
            assert!(owner.is_some());
            assert!(object_type.is_some());
        }
    }
    // Ensure we found the transaction we used for picking the checkpoint to test against
    assert!(found_transaction);
}
