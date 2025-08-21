// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::Blake2b256;
use fastcrypto::merkle::{MerkleNonInclusionProof, MerkleProof, MerkleTree, Node};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::digests::{CheckpointArtifactsDigest, ObjectDigest};
use sui_types::messages_checkpoint::CheckpointArtifacts;
use sui_types::{
    base_types::ObjectID, full_checkpoint_content::CheckpointData,
    messages_checkpoint::VerifiedCheckpoint,
};

use crate::proof::{
    base::{Proof, ProofBuilder, ProofContents, ProofContentsVerifier, ProofTarget},
    error::{ProofError, ProofResult},
};

// To be used for non-inclusion proofs.
// Note that any sequence number, digest will do.
fn get_dummy_object_ref(id: ObjectID) -> ObjectRef {
    (id, SequenceNumber::from_u64(0), ObjectDigest::MIN)
}

/// A target for a proof about the state of an object w.r.t a checkpoint.
/// OCS stands for ObjectCheckpointState (state of object at end of a checkpoint).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OCSTarget {
    // For inclusion proofs, the object ref is the object ref of the object at the end of the checkpoint.
    // For non-inclusion proofs, the object ref is a dummy object ref.
    pub object_ref: ObjectRef,
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
    pub fn new_non_inclusion_target(id: ObjectID) -> Self {
        Self {
            object_ref: get_dummy_object_ref(id),
            target_type: OCSTargetType::NonInclusion,
        }
    }

    pub fn new_inclusion_target(object_ref: ObjectRef) -> Self {
        Self {
            object_ref,
            target_type: OCSTargetType::Inclusion,
        }
    }
}

/// The tree of all objects updated in the checkpoint along with their latest state.
pub struct ModifiedObjectTree {
    pub contents: Vec<ObjectRef>,
    pub tree: MerkleTree<Blake2b256>,
    // Map from object ID to the position of the object in the contents vector.
    pub object_pos_map: HashMap<ObjectID, usize>,
}

impl ModifiedObjectTree {
    pub fn new(artifacts: &CheckpointArtifacts) -> Self {
        let mut object_pos_map = HashMap::new();
        let object_ref_vec = &artifacts.latest_object_states.contents;

        // A sanity check to ensure the object IDs are in increasing order.
        object_ref_vec.windows(2).for_each(|window| {
            let (id1, id2) = (window[0].0, window[1].0);
            if id1 >= id2 {
                panic!(
                    "Object ID {} is not greater than previous object ID {}",
                    id2, id1
                );
            }
        });

        for (i, id) in object_ref_vec.iter().map(|(id, _, _)| id).enumerate() {
            let ret = object_pos_map.insert(*id, i);
            if ret.is_some() {
                panic!("Object ID {} appears more than once", id);
            }
        }
        let tree = MerkleTree::<Blake2b256>::build_from_unserialized(object_ref_vec.iter())
            .expect("Failed to build Merkle tree");
        ModifiedObjectTree {
            contents: object_ref_vec.clone(),
            object_pos_map,
            tree,
        }
    }

    pub fn get_object_state(&self, id: ObjectID) -> Option<&ObjectRef> {
        self.object_pos_map.get(&id).map(|i| &self.contents[*i])
    }

    pub fn is_object_in_checkpoint(&self, id: ObjectID) -> bool {
        self.object_pos_map.contains_key(&id)
    }

    pub fn get_inclusion_proof(&self, object_ref: ObjectRef) -> ProofResult<OCSInclusionProof> {
        // Sanity check: Match ObjectRef with object state in object_map.
        let id = object_ref.0;
        let local_ref = self.get_object_state(id);
        if local_ref.is_none() || local_ref.unwrap() != &object_ref {
            return Err(ProofError::GeneralError(format!(
                "Input object ref {:?} does not match the actual ref {:?}",
                object_ref, local_ref
            )));
        }

        let index = self
            .object_pos_map
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

    pub fn get_non_inclusion_proof(
        &self,
        object_ref: ObjectRef,
    ) -> ProofResult<OCSNonInclusionProof> {
        // Sanity check: Object should not be in checkpoint.
        if self.is_object_in_checkpoint(object_ref.0) {
            return Err(ProofError::GeneralError(format!(
                "Object ID {} is in checkpoint",
                object_ref.0
            )));
        }

        let non_inclusion_proof = self
            .tree
            .compute_non_inclusion_proof(&self.contents, &object_ref)
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

        self.merkle_proof
            .verify_proof_with_unserialized_leaf(
                &Node::from(root.into_inner()),
                &target.object_ref,
                self.leaf_index,
            )
            .map_err(|_| ProofError::InvalidProof)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OCSNonInclusionProof {
    pub non_inclusion_proof: MerkleNonInclusionProof<ObjectRef, Blake2b256>,
}

impl OCSNonInclusionProof {
    pub fn verify(&self, target: &OCSTarget, root: &CheckpointArtifactsDigest) -> ProofResult<()> {
        if target.target_type != OCSTargetType::NonInclusion {
            return Err(ProofError::MismatchedTargetAndProofType);
        }

        self.non_inclusion_proof
            .verify_proof(&Node::from(root.into_inner()), &target.object_ref)
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
                let proof = modified_object_tree.get_inclusion_proof(self.object_ref)?;
                Ok(Proof {
                    targets: ProofTarget::ObjectCheckpointState(self.clone()),
                    checkpoint_summary: checkpoint.checkpoint_summary.clone(),
                    proof_contents: ProofContents::ObjectCheckpointStateProof(OCSProof::Inclusion(
                        proof,
                    )),
                })
            }
            OCSTargetType::NonInclusion => {
                let proof = modified_object_tree.get_non_inclusion_proof(self.object_ref)?;
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
