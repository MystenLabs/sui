// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::RpcError;
use crate::RpcService;
use crate::grpc::alpha::proof_service_proto::{
    GetObjectInclusionProofRequest, GetObjectInclusionProofResponse, OcsInclusionProof,
    proof_service_server::ProofService,
};
use bcs;
use fastcrypto::hash::Blake2b256;
use fastcrypto::merkle::MerkleTree;
use std::str::FromStr;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    digests::Digest,
    messages_checkpoint::CheckpointArtifacts,
};

pub struct ProofServiceImpl {
    service: RpcService,
}

impl ProofServiceImpl {
    pub fn new(service: RpcService) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl ProofService for ProofServiceImpl {
    async fn get_object_inclusion_proof(
        &self,
        request: tonic::Request<GetObjectInclusionProofRequest>,
    ) -> Result<tonic::Response<GetObjectInclusionProofResponse>, tonic::Status> {
        let response = get_object_inclusion_proof_impl(&self.service, request.into_inner())
            .map_err(tonic::Status::from)?;
        Ok(tonic::Response::new(response))
    }
}

fn build_ocs_inclusion_proof(
    checkpoint: &sui_types::full_checkpoint_content::Checkpoint,
    object_id: ObjectID,
    checkpoint_seq: u64,
) -> Result<(OcsInclusionProof, ObjectRef), RpcError> {
    let effects_refs: Vec<&_> = checkpoint
        .transactions
        .iter()
        .map(|tx| &tx.effects)
        .collect();

    let checkpoint_artifacts = CheckpointArtifacts::from(effects_refs.as_slice());

    let object_states = checkpoint_artifacts
        .object_states()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let object_ref_from_checkpoint = object_states.get(&object_id).ok_or_else(|| {
        RpcError::new(
            tonic::Code::FailedPrecondition,
            format!(
                "Object {} was not written at checkpoint {}",
                object_id, checkpoint_seq
            ),
        )
    })?;

    let object_ref = (
        object_id,
        object_ref_from_checkpoint.0,
        object_ref_from_checkpoint.1,
    );

    let leaves: Vec<ObjectRef> = object_states
        .iter()
        .map(|(id, (seq, digest))| (*id, *seq, *digest))
        .collect();

    let leaf_index = leaves
        .iter()
        .position(|r| *r == object_ref)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!("Object {} not found in checkpoint", object_ref.0),
            )
        })?;

    let tree = MerkleTree::<Blake2b256>::build_from_unserialized(leaves.iter())
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let merkle_proof = tree
        .get_proof(leaf_index)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let tree_root = Digest::new(tree.root().bytes());

    let merkle_proof_bytes = bcs::to_bytes(&merkle_proof)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let proto_inclusion_proof = OcsInclusionProof {
        merkle_proof: Some(merkle_proof_bytes),
        leaf_index: Some(leaf_index as u64),
        tree_root: Some(<Digest as AsRef<[u8; 32]>>::as_ref(&tree_root).to_vec()),
    };

    Ok((proto_inclusion_proof, object_ref))
}

#[tracing::instrument(skip(service))]
fn get_object_inclusion_proof_impl(
    service: &RpcService,
    request: GetObjectInclusionProofRequest,
) -> Result<GetObjectInclusionProofResponse, RpcError> {
    if !service.config.authenticated_events_indexing() {
        return Err(RpcError::new(
            tonic::Code::Unimplemented,
            "Authenticated events indexing is disabled".to_string(),
        ));
    }

    let object_id_str = request.object_id.ok_or_else(|| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            "missing object_id".to_string(),
        )
    })?;

    if object_id_str.trim().is_empty() {
        return Err(RpcError::new(
            tonic::Code::InvalidArgument,
            "object_id cannot be empty".to_string(),
        ));
    }

    let object_id = ObjectID::from_str(&object_id_str).map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("invalid object_id: {e}"),
        )
    })?;

    let checkpoint_seq = request.checkpoint.ok_or_else(|| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            "missing checkpoint".to_string(),
        )
    })?;

    let reader = service.reader.inner();
    let indexes = reader.indexes().ok_or_else(RpcError::not_found)?;

    let highest_indexed = indexes
        .get_highest_indexed_checkpoint_seq_number()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?
        .unwrap_or(0);

    let lowest_available = reader
        .get_lowest_available_checkpoint_objects()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    if checkpoint_seq < lowest_available {
        return Err(RpcError::new(
            tonic::Code::NotFound,
            format!(
                "Requested checkpoint {} has been pruned. Lowest available checkpoint is {}",
                checkpoint_seq, lowest_available
            ),
        ));
    }

    if checkpoint_seq > highest_indexed {
        return Err(RpcError::new(
            tonic::Code::NotFound,
            format!(
                "Requested checkpoint {} is not yet indexed. Highest indexed checkpoint is {}",
                checkpoint_seq, highest_indexed
            ),
        ));
    }

    let checkpoint = reader
        .get_checkpoint_by_sequence_number(checkpoint_seq)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::NotFound,
                format!("checkpoint {} not found", checkpoint_seq),
            )
        })?;

    let checkpoint_contents = reader
        .get_checkpoint_contents_by_sequence_number(checkpoint_seq)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::NotFound,
                format!("checkpoint contents for {} not found", checkpoint_seq),
            )
        })?;

    let checkpoint_data = reader
        .get_checkpoint_data(checkpoint, checkpoint_contents)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let (proto_inclusion_proof, object_ref) =
        build_ocs_inclusion_proof(&checkpoint_data, object_id, checkpoint_seq)?;

    let object = reader
        .get_object_by_key(&object_id, object_ref.1)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::NotFound,
                format!("Object {} not found at version {}", object_id, object_ref.1),
            )
        })?;

    debug_assert_eq!(
        object.compute_object_reference().2,
        object_ref.2,
        "Object digest mismatch for object {} at version {}",
        object_id,
        object_ref.1
    );

    let object_data_bytes =
        bcs::to_bytes(&object).map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let mut obj_ref = sui_rpc::proto::sui::rpc::v2::ObjectReference::default();
    obj_ref.object_id = Some(object_ref.0.to_string());
    obj_ref.version = Some(object_ref.1.value());
    obj_ref.digest = Some(object_ref.2.to_string());

    Ok(GetObjectInclusionProofResponse {
        object_ref: Some(obj_ref),
        inclusion_proof: Some(proto_inclusion_proof),
        object_data: Some(object_data_bytes),
    })
}
