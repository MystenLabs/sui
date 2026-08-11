// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::get_checkpoint_request::CheckpointId;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::{
    BatchGetCheckpointsRequest, BatchGetCheckpointsResponse, Checkpoint, ExecutedTransaction,
    GetCheckpointRequest, Object,
};
use test_cluster::TestClusterBuilder;

use crate::{stake_with_validator, transfer_coin};

#[sim_test]
async fn get_checkpoint() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;

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

    let Checkpoint { transactions, .. } = client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths([
                    "sequence_number",
                    "transactions.digest",
                    "transactions.events.events.json",
                ]),
            ),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    let mut found_transaction_with_events = false;
    for tx in transactions {
        if tx.digest == Some(transaction_digest.to_string()) {
            found_transaction_with_events = true;
            let events = tx.events.expect("events should be present");
            assert!(!events.events.is_empty(), "should have events");

            for event in events.events {
                let json = event
                    .json
                    .as_ref()
                    .expect("json field should be populated when requested in mask");

                let prost_types::value::Kind::StructValue(s) =
                    json.kind.as_ref().expect("json should have kind")
                else {
                    panic!("event json should be a struct value");
                };

                let amount = s
                    .fields
                    .get("amount")
                    .and_then(|v| {
                        if let Some(prost_types::value::Kind::StringValue(s)) = &v.kind {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    })
                    .expect("amount should be a string");
                assert_eq!(amount, "30000000000000000");

                let epoch = s
                    .fields
                    .get("epoch")
                    .and_then(|v| {
                        if let Some(prost_types::value::Kind::StringValue(s)) = &v.kind {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    })
                    .expect("epoch should be a string");
                assert_eq!(epoch, "0");

                for addr_field in ["pool_id", "staker_address", "validator_address"] {
                    let addr = s
                        .fields
                        .get(addr_field)
                        .and_then(|v| {
                            if let Some(prost_types::value::Kind::StringValue(s)) = &v.kind {
                                Some(s.as_str())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| panic!("{} should be a string", addr_field));
                    assert!(
                        addr.starts_with("0x"),
                        "{} should start with 0x",
                        addr_field
                    );
                    assert_eq!(
                        addr.len(),
                        66,
                        "{} should be 66 chars (0x + 64 hex)",
                        addr_field
                    );
                }
            }
        }
    }
    assert!(
        found_transaction_with_events,
        "should have found transaction with events"
    );

    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
}

#[sim_test]
async fn batch_get_checkpoints() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;
    let transaction_digest = stake_with_validator(&test_cluster).await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Batch request by sequence number with no provided read_mask
    let BatchGetCheckpointsResponse { checkpoints, .. } = client
        .batch_get_checkpoints({
            let mut message = BatchGetCheckpointsRequest::default();
            message.requests = vec![
                GetCheckpointRequest::by_sequence_number(0),
                GetCheckpointRequest::by_sequence_number(1),
                GetCheckpointRequest::by_sequence_number(2),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(checkpoints.len(), 3);
    for (expected_sequence_number, result) in checkpoints.iter().enumerate() {
        let Checkpoint {
            sequence_number,
            digest,
            summary,
            signature,
            contents,
            transactions,
            objects,
            ..
        } = result.checkpoint();
        assert_eq!(*sequence_number, Some(expected_sequence_number as u64));
        assert!(digest.is_some());
        assert!(summary.is_none());
        assert!(signature.is_none());
        assert!(contents.is_none());
        assert!(transactions.is_empty());
        assert!(objects.is_none());
    }

    let genesis_digest = checkpoints[0].checkpoint().digest.clone().unwrap();

    // Mixed lookups: by digest, by sequence number, and latest
    let BatchGetCheckpointsResponse { checkpoints, .. } = client
        .batch_get_checkpoints({
            let mut message = BatchGetCheckpointsRequest::default();
            message.requests = vec![
                {
                    let mut request = GetCheckpointRequest::default();
                    request.checkpoint_id = Some(CheckpointId::Digest(genesis_digest.clone()));
                    request
                },
                GetCheckpointRequest::by_sequence_number(1),
                GetCheckpointRequest::latest(),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(checkpoints.len(), 3);
    assert_eq!(checkpoints[0].checkpoint().sequence_number, Some(0));
    assert_eq!(
        checkpoints[0].checkpoint().digest,
        Some(genesis_digest.clone())
    );
    assert_eq!(checkpoints[1].checkpoint().sequence_number, Some(1));
    assert!(checkpoints[2].checkpoint().sequence_number.unwrap() >= 1);

    // A missing checkpoint yields a per-entry error without failing the batch
    let BatchGetCheckpointsResponse { checkpoints, .. } = client
        .batch_get_checkpoints({
            let mut message = BatchGetCheckpointsRequest::default();
            message.requests = vec![
                GetCheckpointRequest::by_sequence_number(0),
                GetCheckpointRequest::by_sequence_number(999_999_999),
            ];
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(checkpoints.len(), 2);
    assert_eq!(checkpoints[0].checkpoint().sequence_number, Some(0));
    assert!(checkpoints[1].checkpoint_opt().is_none());
    assert_eq!(checkpoints[1].error().code, tonic::Code::NotFound as i32);

    // A malformed digest fails the whole batch
    let error = client
        .batch_get_checkpoints({
            let mut message = BatchGetCheckpointsRequest::default();
            message.requests = vec![GetCheckpointRequest::by_sequence_number(0), {
                let mut request = GetCheckpointRequest::default();
                request.checkpoint_id = Some(CheckpointId::Digest("not-a-digest".to_owned()));
                request
            }];
            message
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    // Exceeding the batch size limit fails the whole batch
    let error = client
        .batch_get_checkpoints({
            let mut message = BatchGetCheckpointsRequest::default();
            message.requests = vec![GetCheckpointRequest::default(); 101];
            message
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);

    // The top-level read_mask applies to every entry
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

    let BatchGetCheckpointsResponse { checkpoints, .. } = client
        .batch_get_checkpoints({
            let mut message = BatchGetCheckpointsRequest::default();
            message.requests = vec![
                GetCheckpointRequest::by_sequence_number(0),
                GetCheckpointRequest::by_sequence_number(checkpoint),
            ];
            message.read_mask = Some(FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary",
                "contents",
                "transactions.digest",
            ]));
            message
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(checkpoints.len(), 2);
    for result in &checkpoints {
        let Checkpoint {
            sequence_number,
            digest,
            summary,
            signature,
            contents,
            objects,
            ..
        } = result.checkpoint();
        assert!(sequence_number.is_some());
        assert!(digest.is_some());
        assert!(summary.is_some());
        assert!(signature.is_none());
        assert!(contents.is_some());
        assert!(objects.is_none());
    }
    assert!(
        checkpoints[1]
            .checkpoint()
            .transactions
            .iter()
            .any(|t| t.digest == Some(transaction_digest.to_string()))
    );

    tokio::time::sleep(std::time::Duration::from_secs(15)).await;
}
