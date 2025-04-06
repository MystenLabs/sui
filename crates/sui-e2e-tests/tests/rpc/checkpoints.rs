// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::Client as CoreClient;
use sui_rpc_api::field_mask::FieldMask;
use sui_rpc_api::field_mask::FieldMaskUtil;
use sui_rpc_api::proto::node::v2::node_service_client::NodeServiceClient;
use sui_rpc_api::proto::node::v2::{
    FullCheckpointObject, FullCheckpointTransaction, GetCheckpointRequest, GetCheckpointResponse,
    GetFullCheckpointRequest, GetFullCheckpointResponse,
};
use test_cluster::TestClusterBuilder;

use crate::{stake_with_validator, transfer_coin};

#[sim_test]
async fn get_latest_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let core_client = CoreClient::new(test_cluster.rpc_url()).unwrap();

    let _latest = core_client.get_latest_checkpoint().await.unwrap();
}

#[sim_test]
async fn get_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let mut grpc_client = NodeServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request with no provided read_mask
    let GetCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
    } = grpc_client
        .get_checkpoint(GetCheckpointRequest::latest())
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(summary_bcs.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(contents_bcs.is_none());

    // Request all fields
    let response = grpc_client
        .get_checkpoint(
            GetCheckpointRequest::latest().with_read_mask(FieldMask::from_paths([
                "sequence_number",
                "digest",
                "summary",
                "summary_bcs",
                "signature",
                "contents",
                "contents_bcs",
            ])),
        )
        .await
        .unwrap()
        .into_inner();

    let GetCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
    } = &response;

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(summary_bcs.is_some());
    assert!(signature.is_some());
    assert!(contents.is_some());
    assert!(contents_bcs.is_some());

    // Request by digest
    let response = grpc_client
        .get_checkpoint(GetCheckpointRequest::by_digest(digest.clone().unwrap()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.digest, digest.to_owned());

    // Request by sequence_number
    let response = grpc_client
        .get_checkpoint(GetCheckpointRequest::by_sequence_number(
            sequence_number.unwrap(),
        ))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.sequence_number, sequence_number.to_owned());
    assert_eq!(response.digest, digest.to_owned());

    // Request by digest and sequence_number results in an error
    grpc_client
        .get_checkpoint(GetCheckpointRequest {
            sequence_number: Some(sequence_number.unwrap()),
            digest: Some(digest.clone().unwrap()),
            read_mask: None,
        })
        .await
        .unwrap_err();
}

#[sim_test]
async fn get_full_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let transaction_digest = stake_with_validator(&test_cluster).await;

    let core_client = CoreClient::new(test_cluster.rpc_url()).unwrap();

    let latest = core_client
        .get_latest_checkpoint()
        .await
        .unwrap()
        .into_data();
    let _ = core_client
        .get_full_checkpoint(latest.sequence_number)
        .await
        .unwrap();

    let mut grpc_client = NodeServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // A Checkpoint that we know has a transaction that emitted an event
    let checkpoint = grpc_client
        .get_transaction(
            sui_rpc_api::proto::node::v2::GetTransactionRequest::new(transaction_digest)
                .with_read_mask(FieldMask::from_paths(["checkpoint"])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    let GetFullCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
        transactions,
    } = grpc_client
        .get_full_checkpoint(
            GetFullCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths(["sequence_number", "digest", "transactions.digest"]),
            ),
        )
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(summary_bcs.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(contents_bcs.is_none());

    let mut found_transaction = false;
    for FullCheckpointTransaction {
        digest,
        transaction,
        transaction_bcs,
        effects,
        effects_bcs,
        events,
        events_bcs,
        input_objects,
        output_objects,
    } in transactions
    {
        assert!(digest.is_some());
        assert!(transaction.is_none());
        assert!(transaction_bcs.is_none());
        assert!(effects.is_none());
        assert!(effects_bcs.is_none());
        if digest == Some(transaction_digest.into()) {
            found_transaction = true;
        }
        assert!(events.is_none());
        assert!(events_bcs.is_none());
        assert!(input_objects.is_empty());
        assert!(output_objects.is_empty());
    }
    // Ensure we found the transaction we used for picking the checkpoint to test against
    assert!(found_transaction);

    // Request default fields
    let GetFullCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
        transactions,
    } = grpc_client
        .get_full_checkpoint(GetFullCheckpointRequest::by_sequence_number(checkpoint))
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(summary_bcs.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(contents_bcs.is_none());
    assert!(transactions.is_empty());

    // Request all fields
    let response = grpc_client
        .get_full_checkpoint(
            GetFullCheckpointRequest::by_sequence_number(checkpoint).with_read_mask(
                FieldMask::from_paths([
                    "sequence_number",
                    "digest",
                    "summary",
                    "summary_bcs",
                    "signature",
                    "contents",
                    "contents_bcs",
                    "transactions",
                ]),
            ),
        )
        .await
        .unwrap()
        .into_inner();

    let GetFullCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
        transactions,
    } = &response;

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(summary_bcs.is_some());
    assert!(signature.is_some());
    assert!(contents.is_some());
    assert!(contents_bcs.is_some());

    let mut found_transaction = false;
    for FullCheckpointTransaction {
        digest,
        transaction,
        transaction_bcs,
        effects,
        effects_bcs,
        events,
        events_bcs,
        input_objects,
        output_objects,
    } in transactions
    {
        assert!(digest.is_some());
        assert!(transaction.is_some());
        assert!(transaction_bcs.is_some());
        assert!(effects.is_some());
        assert!(effects_bcs.is_some());
        if digest == &Some(transaction_digest.into()) {
            found_transaction = true;
            assert!(events.is_some());
            assert!(events_bcs.is_some());
        }
        assert!(!input_objects.is_empty());
        assert!(!output_objects.is_empty());

        for FullCheckpointObject {
            object_id,
            version,
            digest,
            object,
            object_bcs,
        } in input_objects.iter().chain(output_objects.iter())
        {
            assert!(object_id.is_some());
            assert!(version.is_some());
            assert!(digest.is_some());
            assert!(object.is_some());
            assert!(object_bcs.is_some());
        }
    }
    // Ensure we found the transaction we used for picking the checkpoint to test against
    assert!(found_transaction);

    // Request by digest
    let response = grpc_client
        .get_full_checkpoint(GetFullCheckpointRequest::by_digest(digest.clone().unwrap()))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.digest, digest.to_owned());

    // Request by sequence_number
    let response = grpc_client
        .get_full_checkpoint(GetFullCheckpointRequest::by_sequence_number(
            sequence_number.unwrap(),
        ))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.sequence_number, sequence_number.to_owned());
    assert_eq!(response.digest, digest.to_owned());

    // Request by digest and sequence_number results in an error
    grpc_client
        .get_full_checkpoint(GetFullCheckpointRequest {
            sequence_number: Some(sequence_number.unwrap()),
            digest: Some(digest.clone().unwrap()),
            read_mask: None,
        })
        .await
        .unwrap_err();
}
