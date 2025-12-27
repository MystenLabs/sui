// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authenticated_events::ClientError;
use crate::proof::ocs::OCSInclusionProof;
use std::str::FromStr;
use sui_rpc_api::grpc::alpha::proof_service_proto::OcsInclusionProof as ProtoOcsInclusionProof;
use sui_types::base_types::ObjectID;

pub(super) fn proto_object_ref_to_sui_object_ref(
    proto: &sui_rpc::proto::sui::rpc::v2::ObjectReference,
) -> Result<sui_types::base_types::ObjectRef, ClientError> {
    let object_id_str = proto
        .object_id
        .as_ref()
        .ok_or_else(|| ClientError::InternalError("Missing object_id".to_string()))?;

    let object_id = ObjectID::from_str(object_id_str)
        .map_err(|e| ClientError::InternalError(format!("Invalid object_id: {}", e)))?;

    let version = proto
        .version
        .ok_or_else(|| ClientError::InternalError("Missing version".to_string()))?
        .into();

    let digest_str = proto
        .digest
        .as_ref()
        .ok_or_else(|| ClientError::InternalError("Missing digest".to_string()))?;

    let digest = sui_types::digests::ObjectDigest::from_str(digest_str)
        .map_err(|e| ClientError::InternalError(format!("Invalid digest: {}", e)))?;

    Ok((object_id, version, digest))
}

pub(super) fn proto_ocs_inclusion_proof_to_light_client_proof(
    proto: &ProtoOcsInclusionProof,
) -> Result<OCSInclusionProof, ClientError> {
    let merkle_proof_bytes = proto
        .merkle_proof
        .as_ref()
        .ok_or_else(|| ClientError::InternalError("Missing merkle_proof".to_string()))?;

    let merkle_proof: fastcrypto::merkle::MerkleProof = bcs::from_bytes(merkle_proof_bytes)?;

    let leaf_index = proto
        .leaf_index
        .ok_or_else(|| ClientError::InternalError("Missing leaf_index".to_string()))?
        as usize;

    let tree_root_bytes = proto
        .tree_root
        .as_ref()
        .ok_or_else(|| ClientError::InternalError("Missing tree_root".to_string()))?;

    if tree_root_bytes.len() != 32 {
        return Err(ClientError::InternalError(format!(
            "Invalid tree_root length: {}",
            tree_root_bytes.len()
        )));
    }
    let mut tree_root_arr = [0u8; 32];
    tree_root_arr.copy_from_slice(tree_root_bytes);
    let tree_root = sui_types::digests::Digest::new(tree_root_arr);

    Ok(OCSInclusionProof {
        merkle_proof,
        leaf_index,
        tree_root,
    })
}
