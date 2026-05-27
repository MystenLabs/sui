// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authenticated_events::ClientError;
use crate::proof::ocs::OCSInclusionProof;
use fastcrypto::hash::Blake2b256;
use fastcrypto::merkle::{MerkleProof as FastcryptoMerkleProof, Node};
use std::str::FromStr;
use sui_rpc::proto::sui::rpc::v2alpha::{
    MerkleNode as ProtoMerkleNode, MerkleProof as ProtoMerkleProof,
    OcsInclusionProof as ProtoOcsInclusionProof, merkle_node,
};
use sui_types::base_types::{ObjectID, ObjectRef};

pub(super) fn proto_object_ref_to_sui_object_ref(
    proto: &sui_rpc::proto::sui::rpc::v2::ObjectReference,
) -> Result<ObjectRef, ClientError> {
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

fn proto_merkle_node_to_fastcrypto_node(proto: &ProtoMerkleNode) -> Result<Node, ClientError> {
    let node = proto
        .node
        .as_ref()
        .ok_or_else(|| ClientError::InternalError("MerkleNode missing oneof".to_string()))?;
    match node {
        merkle_node::Node::Empty(_) => Ok(Node::Empty),
        merkle_node::Node::Digest(bytes) => {
            let arr: [u8; 32] = bytes.as_ref().try_into().map_err(|_| {
                ClientError::InternalError(format!(
                    "Invalid merkle node digest length: expected 32 bytes, got {}",
                    bytes.len()
                ))
            })?;
            Ok(Node::Digest(arr))
        }
        _ => Err(ClientError::InternalError(
            "Unknown MerkleNode variant".to_string(),
        )),
    }
}

fn proto_merkle_proof_to_fastcrypto_proof(
    proto: &ProtoMerkleProof,
) -> Result<FastcryptoMerkleProof<Blake2b256>, ClientError> {
    let nodes = proto
        .path
        .iter()
        .map(proto_merkle_node_to_fastcrypto_node)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FastcryptoMerkleProof::new(&nodes))
}

fn proto_tree_root_to_digest(
    bytes: &bytes::Bytes,
) -> Result<sui_types::digests::Digest, ClientError> {
    let arr: [u8; 32] = bytes.as_ref().try_into().map_err(|_| {
        ClientError::InternalError(format!(
            "Invalid tree_root length: expected 32 bytes, got {}",
            bytes.len()
        ))
    })?;
    Ok(sui_types::digests::Digest::new(arr))
}

pub(super) fn proto_ocs_inclusion_proof_to_light_client_proof(
    proto: &ProtoOcsInclusionProof,
) -> Result<OCSInclusionProof, ClientError> {
    let merkle_proof_proto = proto
        .merkle_proof
        .as_ref()
        .ok_or_else(|| ClientError::InternalError("Missing merkle_proof".to_string()))?;
    let merkle_proof = proto_merkle_proof_to_fastcrypto_proof(merkle_proof_proto)?;

    let leaf_index = proto
        .leaf_index
        .ok_or_else(|| ClientError::InternalError("Missing leaf_index".to_string()))?
        as usize;

    let tree_root_bytes = proto
        .tree_root
        .as_ref()
        .ok_or_else(|| ClientError::InternalError("Missing tree_root".to_string()))?;
    let tree_root = proto_tree_root_to_digest(tree_root_bytes)?;

    Ok(OCSInclusionProof {
        merkle_proof,
        leaf_index,
        tree_root,
    })
}
