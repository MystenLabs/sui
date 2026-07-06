// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use mysten_common::backoff::ExponentialBackoff;
use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2alpha::GetCheckpointObjectProofRequest;
use sui_rpc::proto::sui::rpc::v2alpha::GetCheckpointObjectProofResponse;
use sui_rpc::proto::sui::rpc::v2alpha::get_checkpoint_object_proof_response;
use sui_rpc::proto::sui::rpc::v2alpha::proof_service_client::ProofServiceClient;
use sui_types::base_types::ObjectID;
use test_cluster::{TestCluster, TestClusterBuilder};

fn create_rpc_config_with_indexing() -> sui_config::RpcConfig {
    sui_config::RpcConfig {
        enable_indexing: Some(true),
        ..Default::default()
    }
}

async fn get_test_object(test_cluster: &TestCluster) -> ObjectID {
    let sender = test_cluster.wallet.config.keystore.addresses()[0];
    test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap()
        .0
}

/// Retries `request` until the object-proof index catches up to the requested
/// checkpoint and returns the first indexed response. The proof RPC returns
/// NotFound until the checkpoint is indexed (see test_checkpoint_not_yet_indexed),
/// and the index runs behind the fullnode's latest checkpoint, so tests that pick
/// a checkpoint from fullnode state must wait for the index to catch up. Sleeps
/// are virtual under the simulator, so the generous budget costs nothing once the
/// index catches up.
async fn get_object_proof_when_indexed(
    proof_client: &mut ProofServiceClient<tonic::transport::Channel>,
    request: GetCheckpointObjectProofRequest,
) -> GetCheckpointObjectProofResponse {
    const TIMEOUT: Duration = Duration::from_secs(30);
    let mut backoff = ExponentialBackoff::new(Duration::from_millis(100), Duration::from_secs(1));
    tokio::time::timeout(TIMEOUT, async {
        loop {
            match proof_client
                .get_checkpoint_object_proof(request.clone())
                .await
            {
                Ok(response) => return response.into_inner(),
                Err(status)
                    if status.code() == tonic::Code::NotFound
                        && status.message().contains("not yet indexed") =>
                {
                    tokio::time::sleep(backoff.next().unwrap()).await;
                }
                Err(status) => panic!("proof request should succeed once indexed: {status:?}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("object proof index did not catch up within {TIMEOUT:?}"))
}

#[sim_test]
async fn test_missing_object_id() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_indexing())
        .build()
        .await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetCheckpointObjectProofRequest::default().with_checkpoint(1);

    let result = proof_client.get_checkpoint_object_proof(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing object_id"));
}

#[sim_test]
async fn test_empty_object_id() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_indexing())
        .build()
        .await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetCheckpointObjectProofRequest::default()
        .with_object_id("")
        .with_checkpoint(1);

    let result = proof_client.get_checkpoint_object_proof(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("object_id cannot be empty"));
}

#[sim_test]
async fn test_invalid_object_id_format() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_indexing())
        .build()
        .await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetCheckpointObjectProofRequest::default()
        .with_object_id("invalid_object_id")
        .with_checkpoint(1);

    let result = proof_client.get_checkpoint_object_proof(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("invalid object_id"));
}

#[sim_test]
async fn test_missing_checkpoint() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_indexing())
        .build()
        .await;

    let object_id = get_test_object(&test_cluster).await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetCheckpointObjectProofRequest::default().with_object_id(object_id.to_string());

    let result = proof_client.get_checkpoint_object_proof(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing checkpoint"));
}

#[sim_test]
async fn test_checkpoint_not_yet_indexed() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_indexing())
        .build()
        .await;

    let object_id = get_test_object(&test_cluster).await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetCheckpointObjectProofRequest::default()
        .with_object_id(object_id.to_string())
        .with_checkpoint(999999);

    let result = proof_client.get_checkpoint_object_proof(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
    assert!(err.message().contains("not yet indexed"));
}

/// When the object id was not modified in the requested checkpoint, the
/// server returns a non-inclusion proof rather than an error.
#[sim_test]
async fn test_object_not_modified_returns_non_inclusion() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_indexing())
        .build()
        .await;

    let state = test_cluster.fullnode_handle.sui_node.state();
    let latest_checkpoint = state.get_latest_checkpoint_sequence_number().unwrap();

    let non_existent_object_id = ObjectID::random();

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetCheckpointObjectProofRequest::default()
        .with_object_id(non_existent_object_id.to_string())
        .with_checkpoint(latest_checkpoint);

    let response = get_object_proof_when_indexed(&mut proof_client, request).await;

    let proof = response.proof.expect("proof should be present");
    assert!(
        matches!(
            proof,
            get_checkpoint_object_proof_response::Proof::NonInclusion(_)
        ),
        "expected non-inclusion proof for a random object id"
    );
}

#[sim_test]
async fn test_valid_request() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_indexing())
        .build()
        .await;

    let object_id = get_test_object(&test_cluster).await;

    let state = test_cluster.fullnode_handle.sui_node.state();
    let latest_checkpoint = state.get_latest_checkpoint_sequence_number().unwrap();

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Walk forward through checkpoints until we find one that modified the
    // target object — the server returns inclusion in that case and
    // non-inclusion otherwise.
    let mut found = None;
    for checkpoint_seq in 0..=latest_checkpoint {
        let request = GetCheckpointObjectProofRequest::default()
            .with_object_id(object_id.to_string())
            .with_checkpoint(checkpoint_seq);
        let response = get_object_proof_when_indexed(&mut proof_client, request).await;

        if let Some(get_checkpoint_object_proof_response::Proof::Inclusion(p)) =
            response.proof.as_ref()
        {
            found = Some((checkpoint_seq, p.clone(), response.clone()));
            break;
        }
    }

    let (checkpoint_seq, inclusion_proof, response) =
        found.expect("Object should be modified in at least one checkpoint");

    let object_ref = inclusion_proof
        .object_ref
        .as_ref()
        .expect("object_ref should be present");
    assert!(
        object_ref.object_id.is_some(),
        "object_id should be present"
    );
    assert!(object_ref.version.is_some(), "version should be present");
    assert!(object_ref.digest.is_some(), "digest should be present");

    assert!(
        inclusion_proof.merkle_proof.is_some(),
        "merkle_proof should be present"
    );
    assert!(
        inclusion_proof.leaf_index.is_some(),
        "leaf_index should be present"
    );
    assert!(
        inclusion_proof.tree_root.is_some(),
        "tree_root should be present"
    );

    assert!(
        inclusion_proof.object_data.is_some(),
        "object_data should be present for a live object"
    );

    assert!(
        response.checkpoint_summary.is_some(),
        "checkpoint_summary should be present"
    );

    let _ = checkpoint_seq;
}
