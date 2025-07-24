// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::Blake2b256;
use fastcrypto::merkle::{MerkleNonInclusionProof, MerkleProof, MerkleTree, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sui_types::digests::ObjectDigest;
use sui_types::messages_checkpoint::{
    CheckpointArtifacts, CheckpointArtifactsDigest, ObjectCheckpointState,
};
use sui_types::{
    base_types::ObjectID, full_checkpoint_content::CheckpointData,
    messages_checkpoint::VerifiedCheckpoint,
};

use crate::proof::{
    base::{Proof, ProofBuilder, ProofContents, ProofContentsVerifier, ProofTarget},
    error::{ProofError, ProofResult},
};

/// A target for a proof about the state of an object w.r.t a checkpoint.
/// OCS stands for ObjectCheckpointState
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OCSTarget {
    pub id: ObjectID,
    pub digest: Option<ObjectDigest>,
    pub target_type: OCSTargetType,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum OCSTargetType {
    /// A proof that state of an object o is d at checkpoint c. For cases where o is updated in c.
    Inclusion,
    /// A proof that o was not updated in c. For cases where o is not updated in c.
    NonInclusion,
}

impl OCSTarget {
    pub fn new(
        id: ObjectID,
        digest: Option<ObjectDigest>,
        proof_type: OCSTargetType,
    ) -> ProofResult<Self> {
        // If the proof type is non-inclusion, the digest must be none.
        // Note that if the proof type is inclusion, the digest can be either
        // some or none (if the object got deleted in the checkpoint).
        if proof_type == OCSTargetType::NonInclusion && digest.is_some() {
            return Err(ProofError::MismatchedTargetAndProofType);
        }
        Ok(Self {
            id,
            digest,
            target_type: proof_type,
        })
    }

    pub fn new_non_inclusion_target(id: ObjectID) -> Self {
        Self {
            id,
            digest: None,
            target_type: OCSTargetType::NonInclusion,
        }
    }

    pub fn new_inclusion_target(object_state: &ObjectCheckpointState) -> Self {
        Self {
            id: object_state.id,
            digest: object_state.digest,
            target_type: OCSTargetType::Inclusion,
        }
    }
}

/// The tree of all objects updated in the checkpoint along with their latest state.
pub struct ModifiedObjectTree {
    pub contents: Vec<ObjectCheckpointState>,
    pub tree: MerkleTree<Blake2b256>,
    pub object_map: HashMap<ObjectID, usize>,
}

impl ModifiedObjectTree {
    pub fn new(artifacts: &CheckpointArtifacts) -> Self {
        let mut object_map = HashMap::new();
        let mut prev_obj_id = ObjectID::from_hex_literal("0x0").unwrap();
        for (i, object_state) in artifacts.latest_object_states.contents.iter().enumerate() {
            // A sanity check to ensure the object IDs are in increasing order.
            if object_state.id <= prev_obj_id {
                panic!(
                    "Object ID {} is not greater than previous object ID {}",
                    object_state.id, prev_obj_id
                );
            } else {
                prev_obj_id = object_state.id;
            }

            let ret = object_map.insert(object_state.id, i);
            if ret.is_some() {
                panic!("Object ID {} appears more than once", object_state.id);
            }
        }
        let contents = artifacts.latest_object_states.contents.clone();
        let tree = MerkleTree::<Blake2b256>::build_from_unserialized(contents.iter())
            .expect("Failed to build Merkle tree");
        ModifiedObjectTree {
            contents,
            object_map,
            tree,
        }
    }

    pub fn get_object_state(&self, id: ObjectID) -> Option<&ObjectCheckpointState> {
        self.object_map.get(&id).map(|i| &self.contents[*i])
    }

    pub fn is_object_in_checkpoint(&self, id: ObjectID) -> bool {
        self.object_map.contains_key(&id)
    }

    pub fn get_inclusion_proof(&self, id: ObjectID) -> ProofResult<OCSInclusionProof> {
        let index = self
            .object_map
            .get(&id)
            .ok_or(ProofError::GeneralError(format!(
                "Object ID {} not found",
                id
            )))?;
        let proof = self
            .tree
            .get_proof(*index)
            .map_err(|e| ProofError::GeneralError(e.to_string()))?;
        Ok(OCSInclusionProof {
            merkle_proof: proof,
            leaf_index: *index,
        })
    }

    pub fn get_non_inclusion_proof(&self, id: ObjectID) -> ProofResult<OCSNonInclusionProof> {
        if self.is_object_in_checkpoint(id) {
            return Err(ProofError::GeneralError(format!(
                "Object ID {} is in checkpoint",
                id
            )));
        }
        let target_object_state = ObjectCheckpointState::new(id, None);
        let non_inclusion_proof = self
            .tree
            .compute_non_inclusion_proof(&self.contents, &target_object_state)
            .map_err(|e| ProofError::GeneralError(e.to_string()))?;
        Ok(OCSNonInclusionProof {
            non_inclusion_proof,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OCSInclusionProof {
    pub merkle_proof: MerkleProof<Blake2b256>,
    pub leaf_index: usize,
}

impl OCSInclusionProof {
    pub fn verify(&self, target: &OCSTarget, root: &CheckpointArtifactsDigest) -> ProofResult<()> {
        if target.target_type != OCSTargetType::Inclusion {
            return Err(ProofError::MismatchedTargetAndProofType);
        }
        let object_state = ObjectCheckpointState::new(target.id, target.digest);

        self.merkle_proof
            .verify_proof_with_unserialized_leaf(
                &Node::from(root.digest.into_inner()),
                &object_state,
                self.leaf_index,
            )
            .map_err(|_| ProofError::InvalidProof)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OCSNonInclusionProof {
    pub non_inclusion_proof: MerkleNonInclusionProof<ObjectCheckpointState, Blake2b256>,
}

impl OCSNonInclusionProof {
    pub fn verify(&self, target: &OCSTarget, root: &CheckpointArtifactsDigest) -> ProofResult<()> {
        if target.target_type != OCSTargetType::NonInclusion {
            return Err(ProofError::MismatchedTargetAndProofType);
        }
        if target.digest.is_some() {
            return Err(ProofError::GeneralError(
                "Target digest is not none for non-inclusion proof".to_string(),
            ));
        }

        self.non_inclusion_proof
            .verify_proof(
                &Node::from(root.digest.into_inner()),
                &ObjectCheckpointState::new(target.id, None),
            )
            .map_err(|_| ProofError::InvalidProof)
    }
}

/// A proof about the state of an object w.r.t a checkpoint.
#[derive(Debug, Serialize, Deserialize)]
pub enum OCSProof {
    /// Proof for OCSTargetType::Inclusion
    Inclusion(OCSInclusionProof),
    /// Proof for OCSTargetType::NonInclusion
    NonInclusion(OCSNonInclusionProof),
}

impl ProofContentsVerifier for OCSProof {
    fn verify(self, target: &ProofTarget, summary: &VerifiedCheckpoint) -> ProofResult<()> {
        match target {
            ProofTarget::ObjectCheckpointState(target) => {
                let artifacts_digest = summary
                    .data()
                    .checkpoint_artifacts_digest()
                    .map_err(|e| ProofError::GeneralError(e.to_string()))?;
                match self {
                    OCSProof::Inclusion(proof) => proof.verify(target, artifacts_digest),
                    OCSProof::NonInclusion(proof) => proof.verify(target, artifacts_digest),
                }
            }
            _ => Err(ProofError::MismatchedTargetAndProofType),
        }
    }
}

impl ProofBuilder for OCSTarget {
    fn construct(self, checkpoint: &CheckpointData) -> ProofResult<Proof> {
        let artifacts = CheckpointArtifacts::from(checkpoint);
        let modified_object_tree = ModifiedObjectTree::new(&artifacts);
        match self.target_type {
            OCSTargetType::Inclusion => {
                let proof = modified_object_tree.get_inclusion_proof(self.id)?;
                Ok(Proof {
                    targets: ProofTarget::ObjectCheckpointState(self.clone()),
                    checkpoint_summary: checkpoint.checkpoint_summary.clone(),
                    proof_contents: ProofContents::ObjectCheckpointStateProof(OCSProof::Inclusion(
                        proof,
                    )),
                })
            }
            OCSTargetType::NonInclusion => {
                let proof = modified_object_tree.get_non_inclusion_proof(self.id)?;
                Ok(Proof {
                    targets: ProofTarget::ObjectCheckpointState(self.clone()),
                    checkpoint_summary: checkpoint.checkpoint_summary.clone(),
                    proof_contents: ProofContents::ObjectCheckpointStateProof(
                        OCSProof::NonInclusion(proof),
                    ),
                })
            }
        }
    }
}
