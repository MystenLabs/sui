// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::client::ResponseExt;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_sdk_types::Digest;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_service_info() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut grpc_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
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

#[sim_test]
async fn chain_id_is_base58_digest() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let chain_identifier = test_cluster.get_chain_identifier();
    let expected_digest = Digest::new(chain_identifier.as_bytes().to_owned());

    let mut grpc_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let response = grpc_client
        .get_service_info(GetServiceInfoRequest::default())
        .await
        .unwrap();

    // Verify the X-Sui-Chain-Id response header is a base58-encoded
    // 32-byte digest matching the genesis checkpoint digest.
    let header_chain_id = response.chain_id().expect("missing x-sui-chain-id header");
    assert_eq!(header_chain_id, expected_digest);

    // Verify the chain_id field in the GetServiceInfo response body
    // also matches.
    let body_chain_id = response.into_inner().chain_id.unwrap();
    assert_eq!(body_chain_id, expected_digest.to_string());

    // Fetch checkpoint 0 (genesis) and verify its digest matches the
    // chain id.
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
        .expect("digest should be valid base58");
    assert_eq!(genesis_digest, expected_digest);
}
