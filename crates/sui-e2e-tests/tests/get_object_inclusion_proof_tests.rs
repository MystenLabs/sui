// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_keys::keystore::AccountKeystore;
use sui_macros::sim_test;
use sui_rpc_api::grpc::alpha::proof_service_proto::GetObjectInclusionProofRequest;
use sui_rpc_api::grpc::alpha::proof_service_proto::proof_service_client::ProofServiceClient;
use sui_types::base_types::ObjectID;
use test_cluster::{TestCluster, TestClusterBuilder};

fn create_rpc_config_with_authenticated_events() -> sui_config::RpcConfig {
    sui_config::RpcConfig {
        authenticated_events_indexing: Some(true),
        enable_indexing: Some(true),
        ..Default::default()
    }
}

fn create_rpc_config_without_authenticated_events() -> sui_config::RpcConfig {
    sui_config::RpcConfig {
        authenticated_events_indexing: Some(false),
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

#[sim_test]
async fn test_feature_flag_disabled() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_without_authenticated_events())
        .build()
        .await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some(ObjectID::random().to_string());
    req.checkpoint = Some(1);

    let result = proof_client.get_object_inclusion_proof(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unimplemented);
    assert!(
        err.message()
            .contains("Authenticated events indexing is disabled")
    );
}

#[sim_test]
async fn test_missing_object_id() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_authenticated_events())
        .build()
        .await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = None;
    req.checkpoint = Some(1);

    let result = proof_client.get_object_inclusion_proof(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing object_id"));
}

#[sim_test]
async fn test_empty_object_id() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_authenticated_events())
        .build()
        .await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some("".to_string());
    req.checkpoint = Some(1);

    let result = proof_client.get_object_inclusion_proof(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("object_id cannot be empty"));
}

#[sim_test]
async fn test_invalid_object_id_format() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_authenticated_events())
        .build()
        .await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some("invalid_object_id".to_string());
    req.checkpoint = Some(1);

    let result = proof_client.get_object_inclusion_proof(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("invalid object_id"));
}

#[sim_test]
async fn test_missing_checkpoint() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_authenticated_events())
        .build()
        .await;

    let object_id = get_test_object(&test_cluster).await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some(object_id.to_string());
    req.checkpoint = None;

    let result = proof_client.get_object_inclusion_proof(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(err.message().contains("missing checkpoint"));
}

#[sim_test]
async fn test_checkpoint_not_yet_indexed() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_authenticated_events())
        .build()
        .await;

    let object_id = get_test_object(&test_cluster).await;

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some(object_id.to_string());
    req.checkpoint = Some(999999);

    let result = proof_client.get_object_inclusion_proof(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
    assert!(err.message().contains("not yet indexed"));
}

#[sim_test]
async fn test_object_not_found_in_checkpoint() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_authenticated_events())
        .build()
        .await;

    let state = test_cluster.fullnode_handle.sui_node.state();
    let latest_checkpoint = state.get_latest_checkpoint_sequence_number().unwrap();

    let non_existent_object_id = ObjectID::random();

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some(non_existent_object_id.to_string());
    req.checkpoint = Some(latest_checkpoint);

    let result = proof_client.get_object_inclusion_proof(req).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    assert!(err.message().contains("was not written at checkpoint"));
}

#[sim_test]
async fn test_valid_request() {
    let test_cluster = TestClusterBuilder::new()
        .disable_fullnode_pruning()
        .with_rpc_config(create_rpc_config_with_authenticated_events())
        .build()
        .await;

    let object_id = get_test_object(&test_cluster).await;

    let state = test_cluster.fullnode_handle.sui_node.state();
    let latest_checkpoint = state.get_latest_checkpoint_sequence_number().unwrap();

    let mut proof_client = ProofServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut found_checkpoint = None;
    for checkpoint_seq in 0..=latest_checkpoint {
        let mut req = GetObjectInclusionProofRequest::default();
        req.object_id = Some(object_id.to_string());
        req.checkpoint = Some(checkpoint_seq);

        let result = proof_client.get_object_inclusion_proof(req).await;
        if result.is_ok() {
            found_checkpoint = Some(checkpoint_seq);
            break;
        }
    }

    assert!(
        found_checkpoint.is_some(),
        "Object not found in any checkpoint"
    );
    let checkpoint_seq = found_checkpoint.unwrap();

    let mut req = GetObjectInclusionProofRequest::default();
    req.object_id = Some(object_id.to_string());
    req.checkpoint = Some(checkpoint_seq);

    let result = proof_client.get_object_inclusion_proof(req).await;
    assert!(result.is_ok());
    let response = result.unwrap().into_inner();

    let object_ref = response.object_ref.expect("object_ref should be present");
    assert!(
        object_ref.object_id.is_some(),
        "object_id should be present"
    );
    assert!(object_ref.version.is_some(), "version should be present");
    assert!(object_ref.digest.is_some(), "digest should be present");

    let inclusion_proof = response
        .inclusion_proof
        .expect("inclusion_proof should be present");
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
        response.object_data.is_some(),
        "object_data should be present"
    );
}
