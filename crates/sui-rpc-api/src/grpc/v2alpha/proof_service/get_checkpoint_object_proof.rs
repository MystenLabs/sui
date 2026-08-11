// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use sui_rpc::proto::sui::rpc::v2alpha::GetCheckpointObjectProofRequest;
use sui_rpc::proto::sui::rpc::v2alpha::GetCheckpointObjectProofResponse;
use sui_rpc::proto::sui::rpc::v2alpha::OcsInclusionProof as ProtoOcsInclusionProof;
use sui_rpc::proto::sui::rpc::v2alpha::OcsNonInclusionProof as ProtoOcsNonInclusionProof;
use sui_rpc::proto::sui::rpc::v2alpha::get_checkpoint_object_proof_response;
use sui_sdk_types::ObjectReference as SdkObjectReference;
use sui_sdk_types::merkle::MerkleTree;
use sui_sdk_types::proof::OcsInclusionProof as SdkOcsInclusionProof;
use sui_sdk_types::proof::OcsNonInclusionProof as SdkOcsNonInclusionProof;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::effects::TransactionEffects;
use sui_types::full_checkpoint_content::Checkpoint as FullCheckpoint;
use sui_types::messages_checkpoint::CheckpointArtifacts;

use crate::RpcError;
use crate::RpcService;

#[tracing::instrument(skip(service))]
pub(super) fn get_checkpoint_object_proof(
    service: &RpcService,
    request: GetCheckpointObjectProofRequest,
) -> Result<GetCheckpointObjectProofResponse, RpcError> {
    let object_id = parse_object_id(&request)?;
    let checkpoint_seq = request.checkpoint.ok_or_else(|| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            "missing checkpoint".to_string(),
        )
    })?;
    validate_checkpoint_bounds(service, checkpoint_seq)?;

    let checkpoint_data = load_checkpoint(service, checkpoint_seq)?;
    let ocs_leaves = build_ocs_leaves(&checkpoint_data, checkpoint_seq)?;

    let checkpoint_summary = bcs::to_bytes(&checkpoint_data.summary)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let proof = match ocs_leaves.binary_search_by_key(&object_id, |r| r.0) {
        Ok(leaf_index) => build_inclusion_proof(service, &ocs_leaves, leaf_index)?,
        Err(_) => build_non_inclusion_proof(&ocs_leaves, object_id)?,
    };

    let mut response = GetCheckpointObjectProofResponse::default();
    response.checkpoint_summary = Some(checkpoint_summary.into());
    response.proof = Some(proof);
    Ok(response)
}

fn parse_object_id(request: &GetCheckpointObjectProofRequest) -> Result<ObjectID, RpcError> {
    let object_id_str = request.object_id.as_ref().ok_or_else(|| {
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
    ObjectID::from_str(object_id_str).map_err(|e| {
        RpcError::new(
            tonic::Code::InvalidArgument,
            format!("invalid object_id: {e}"),
        )
    })
}

fn validate_checkpoint_bounds(service: &RpcService, checkpoint_seq: u64) -> Result<(), RpcError> {
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
                "requested checkpoint {checkpoint_seq} has been pruned; lowest available checkpoint \
                 is {lowest_available}",
            ),
        ));
    }
    if checkpoint_seq > highest_indexed {
        return Err(RpcError::new(
            tonic::Code::NotFound,
            format!(
                "requested checkpoint {checkpoint_seq} is not yet indexed; highest indexed \
                 checkpoint is {highest_indexed}",
            ),
        ));
    }
    Ok(())
}

fn load_checkpoint(service: &RpcService, checkpoint_seq: u64) -> Result<FullCheckpoint, RpcError> {
    let reader = service.reader.inner();
    let checkpoint = reader
        .get_checkpoint_by_sequence_number(checkpoint_seq)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::NotFound,
                format!("checkpoint {checkpoint_seq} not found"),
            )
        })?;
    let checkpoint_contents = reader
        .get_checkpoint_contents_by_sequence_number(checkpoint_seq)
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::NotFound,
                format!("checkpoint contents for {checkpoint_seq} not found"),
            )
        })?;
    reader
        .get_checkpoint_data(checkpoint, checkpoint_contents)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))
}

/// Build the OCS Merkle leaves for `checkpoint`. Leaves are
/// `(ObjectID, SequenceNumber, ObjectDigest)` tuples from the checkpoint's
/// modified-object set, sorted by ObjectID (the order
/// [`CheckpointArtifacts::object_states`] already returns them in, since the
/// underlying map is a `BTreeMap` keyed by `ObjectID`).
fn build_ocs_leaves(
    checkpoint: &FullCheckpoint,
    checkpoint_seq: u64,
) -> Result<Vec<ObjectRef>, RpcError> {
    let effects_refs: Vec<&TransactionEffects> = checkpoint
        .transactions
        .iter()
        .map(|tx| &tx.effects)
        .collect();

    // V1 effects predate the OCS commitment, so checkpoints that contain any
    // V1 effects cannot be authenticated against a checkpoint-artifacts
    // digest. Reject up front rather than producing a proof that wouldn't
    // verify.
    if effects_refs
        .iter()
        .any(|effects| matches!(effects, TransactionEffects::V1(_)))
    {
        return Err(RpcError::new(
            tonic::Code::FailedPrecondition,
            format!(
                "object proofs are not supported for checkpoint {checkpoint_seq} because it \
                 contains TransactionEffectsV1",
            ),
        ));
    }

    let artifacts = CheckpointArtifacts::from(effects_refs.as_slice());
    let object_states = artifacts
        .object_states()
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    Ok(object_states
        .iter()
        .map(|(id, (version, digest))| (*id, *version, *digest))
        .collect())
}

fn build_inclusion_proof(
    service: &RpcService,
    ocs_leaves: &[ObjectRef],
    leaf_index: usize,
) -> Result<get_checkpoint_object_proof_response::Proof, RpcError> {
    let object_ref = ocs_leaves[leaf_index];
    let sdk_leaves = sdk_object_refs(ocs_leaves);
    let tree = MerkleTree::build_from_unserialized(&sdk_leaves)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    let merkle_proof = tree
        .get_proof(leaf_index)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    let tree_root = sui_sdk_types::Digest::new(tree.root().bytes());

    let sdk_proof = SdkOcsInclusionProof {
        merkle_proof,
        leaf_index: leaf_index as u64,
        tree_root,
    };

    let mut proto_proof: ProtoOcsInclusionProof = (&sdk_proof).into();
    proto_proof.object_ref = Some(sdk_leaves[leaf_index].clone().into());

    // Carry the live object's BCS-encoded `Object` next to the inclusion
    // proof so verifiers don't need a separate round-trip. Deletions and
    // wraps are signalled by the leaf digest sentinel and have no live
    // object to return.
    if object_ref.2.is_alive() {
        let reader = service.reader.inner();
        let object = reader
            .get_object_by_key(&object_ref.0, object_ref.1)
            .ok_or_else(|| {
                RpcError::new(
                    tonic::Code::NotFound,
                    format!(
                        "object {} not found at version {}",
                        object_ref.0, object_ref.1
                    ),
                )
            })?;
        debug_assert_eq!(
            object.compute_object_reference().2,
            object_ref.2,
            "object digest mismatch for object {} at version {}",
            object_ref.0,
            object_ref.1
        );
        let object_data = bcs::to_bytes(&object)
            .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
        proto_proof.object_data = Some(object_data.into());
    }

    Ok(get_checkpoint_object_proof_response::Proof::Inclusion(
        proto_proof,
    ))
}

fn build_non_inclusion_proof(
    ocs_leaves: &[ObjectRef],
    object_id: ObjectID,
) -> Result<get_checkpoint_object_proof_response::Proof, RpcError> {
    let sdk_leaves = sdk_object_refs(ocs_leaves);
    let tree = MerkleTree::build_from_unserialized(&sdk_leaves)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    // The target only needs the object id to be correct: the non-inclusion
    // search keys on the leaf's BCS sort key, and (object_id, _, _) sorts
    // exactly where any leaf with that id would sort regardless of the
    // version/digest bytes that follow.
    let target = SdkObjectReference::new(object_id.into(), 0, sui_sdk_types::Digest::new([0; 32]));
    let non_inclusion = tree
        .compute_non_inclusion_proof(&sdk_leaves, &target)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
    let tree_root = sui_sdk_types::Digest::new(tree.root().bytes());

    let sdk_proof = SdkOcsNonInclusionProof {
        non_inclusion_proof: non_inclusion,
        tree_root,
    };
    let proto_proof: ProtoOcsNonInclusionProof = (&sdk_proof).into();
    Ok(get_checkpoint_object_proof_response::Proof::NonInclusion(
        proto_proof,
    ))
}

fn sdk_object_refs(refs: &[ObjectRef]) -> Vec<SdkObjectReference> {
    refs.iter()
        .map(|(id, version, digest)| {
            SdkObjectReference::new((*id).into(), version.value(), (*digest).into())
        })
        .collect()
}
