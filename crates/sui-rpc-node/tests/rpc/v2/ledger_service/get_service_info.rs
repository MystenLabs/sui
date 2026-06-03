// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors `sui-e2e-tests/tests/rpc/v2/ledger_service/get_service_info.rs`.
//! `get_service_info` is the cheapest read endpoint, so this is
//! also the cluster's smoke test: it proves that the harness
//! stood up an indexer + RPC server reachable on its ephemeral
//! port without any prior `create_checkpoint`.

use sui_rpc::client::ResponseExt;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::Digest;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn get_service_info() {
    let cluster = LocalCluster::new().await.unwrap();

    let mut grpc_client: LedgerServiceClient<Channel> =
        LedgerServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let GetServiceInfoResponse {
        chain_id,
        chain,
        epoch,
        checkpoint_height,
        timestamp,
        lowest_available_checkpoint,
        lowest_available_checkpoint_objects,
        server,
        ..
    } = grpc_client
        .get_service_info(GetServiceInfoRequest::default())
        .await
        .unwrap()
        .into_inner();

    assert!(chain_id.is_some());
    assert!(chain.is_some());
    assert!(epoch.is_some());
    assert!(checkpoint_height.is_some());
    assert!(timestamp.is_some());
    assert!(lowest_available_checkpoint.is_some());
    assert!(lowest_available_checkpoint_objects.is_some());
    assert!(server.is_some());
}

/// Verify the X-Sui-Chain-Id header and the body `chain_id` field
/// both surface the base58-encoded genesis checkpoint digest, and
/// that fetching checkpoint 0 returns the matching digest.
#[tokio::test]
async fn chain_id_is_base58_digest() {
    let cluster = LocalCluster::new().await.unwrap();

    let mut grpc_client: LedgerServiceClient<Channel> =
        LedgerServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let response = grpc_client
        .get_service_info(GetServiceInfoRequest::default())
        .await
        .unwrap();

    let header_chain_id = response.chain_id().expect("missing x-sui-chain-id header");
    let body_chain_id: Digest = response
        .into_inner()
        .chain_id
        .unwrap()
        .parse()
        .expect("body chain_id should parse as a base58 Digest");
    assert_eq!(header_chain_id, body_chain_id);

    let genesis = grpc_client
        .get_checkpoint(GetCheckpointRequest::by_sequence_number(0))
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap();

    let genesis_digest: Digest = genesis
        .digest
        .expect("genesis checkpoint should have a digest")
        .parse()
        .expect("checkpoint digest should be valid base58");
    assert_eq!(genesis_digest, body_chain_id);
}
