// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::Blake2b256;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::digests::{CheckpointArtifactsDigest, Digest, ObjectDigest};
use sui_types::messages_checkpoint::CheckpointArtifacts;
use sui_types::{
    base_types::ObjectID, full_checkpoint_content::CheckpointData,
    messages_checkpoint::VerifiedCheckpoint,
};

use crate::proof::{
    base::{Proof, ProofBuilder, ProofContents, ProofContentsVerifier, ProofTarget},
    error::{ProofError, ProofResult},
};

type MerkleTree = fastcrypto::merkle::MerkleTree<Blake2b256>;
type MerkleNonInclusionProof = fastcrypto::merkle::MerkleNonInclusionProof<ObjectRef, Blake2b256>;
type MerkleProof = fastcrypto::merkle::MerkleProof<Blake2b256>;
type Node = fastcrypto::merkle::Node;
type ArtifactDigest = Digest;

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
            object_ref: Self::get_dummy_object_ref(id),
            target_type: OCSTargetType::NonInclusion,
        }
    }

    pub fn new_inclusion_target(object_ref: ObjectRef) -> Self {
        Self {
            object_ref,
            target_type: OCSTargetType::Inclusion,
        }
    }

    // To be used for non-inclusion proofs.
    // Note that any sequence number, digest will do.
    fn get_dummy_object_ref(id: ObjectID) -> ObjectRef {
        (id, SequenceNumber::from_u64(0), ObjectDigest::MIN)
    }
}

#[derive(Debug)]
/// The tree of all objects updated in the checkpoint along with their states.
pub struct ModifiedObjectTree {
    // The leaves of the Merkle tree.
    pub leaves: Vec<ObjectRef>,
    // The Merkle tree built from the leaves.
    pub tree: MerkleTree,
    // The root of the Merkle tree.
    pub tree_root: ArtifactDigest,
    // Map from object ID to the position of the object in the leaves vector.
    pub object_pos_map: HashMap<ObjectID, usize>,
}

impl ModifiedObjectTree {
    pub fn new(artifacts: &CheckpointArtifacts) -> ProofResult<Self> {
        let mut object_pos_map = HashMap::new();
        let object_states = artifacts
            .object_states()
            .map_err(|e| ProofError::GeneralError(e.to_string()))?;
        let leaves = object_states
            .iter()
            .map(|(id, (seq, digest))| (*id, *seq, *digest))
            .collect::<Vec<_>>();

        // Create a map from object ID to the position of the object in the leaves vector.
        for (i, id) in leaves.iter().map(|(id, _, _)| id).enumerate() {
            let ret = object_pos_map.insert(*id, i);

            // Sanity check: Object ID should not appear more than once.
            if ret.is_some() {
                return Err(ProofError::GeneralError(format!(
                    "Object ID {} appears more than once",
                    id
                )));
            }
        }

        // Build the Merkle tree from the leaves.
        let tree = MerkleTree::build_from_unserialized(leaves.iter())
            .map_err(|e| ProofError::GeneralError(e.to_string()))?;
        let tree_root = Digest::new(tree.root().bytes());

        Ok(ModifiedObjectTree {
            leaves,
            object_pos_map,
            tree,
            tree_root,
        })
    }

    pub fn get_object_state(&self, id: ObjectID) -> Option<&ObjectRef> {
        self.object_pos_map.get(&id).map(|i| &self.leaves[*i])
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

        // Get the index of the object in the leaves vector.
        let index = self
            .object_pos_map
            .get(&id)
            .ok_or(ProofError::GeneralError(format!(
                "Object ID {} not found",
                id
            )))?;

        // Get the Merkle proof for the object.
        let proof = self
            .tree
            .get_proof(*index)
            .map_err(|e| ProofError::GeneralError(e.to_string()))?;
        Ok(OCSInclusionProof {
            merkle_proof: proof,
            leaf_index: *index,
            tree_root: self.tree_root,
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

        // Get the Merkle non-inclusion proof for the object.
        let non_inclusion_proof = self
            .tree
            .compute_non_inclusion_proof(&self.leaves, &object_ref)
            .map_err(|e| ProofError::GeneralError(e.to_string()))?;
        Ok(OCSNonInclusionProof {
            non_inclusion_proof,
            tree_root: self.tree_root,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OCSInclusionProof {
    pub merkle_proof: MerkleProof,
    pub leaf_index: usize,
    pub tree_root: ArtifactDigest,
}

impl OCSInclusionProof {
    pub fn verify(&self, target: &OCSTarget) -> ProofResult<()> {
        if target.target_type != OCSTargetType::Inclusion {
            return Err(ProofError::MismatchedTargetAndProofType);
        }

        self.merkle_proof
            .verify_proof_with_unserialized_leaf(
                &Node::from(self.tree_root.into_inner()),
                &target.object_ref,
                self.leaf_index,
            )
            .map_err(|_| ProofError::InvalidProof)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OCSNonInclusionProof {
    pub non_inclusion_proof: MerkleNonInclusionProof,
    pub tree_root: ArtifactDigest,
}

impl OCSNonInclusionProof {
    pub fn verify(&self, target: &OCSTarget) -> ProofResult<()> {
        if target.target_type != OCSTargetType::NonInclusion {
            return Err(ProofError::MismatchedTargetAndProofType);
        }

        self.non_inclusion_proof
            .verify_proof(&Node::from(self.tree_root.into_inner()), &target.object_ref)
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

impl OCSProof {
    pub fn get_artifact_digest(&self) -> ArtifactDigest {
        match self {
            OCSProof::Inclusion(proof) => proof.tree_root,
            OCSProof::NonInclusion(proof) => proof.tree_root,
        }
    }
}

impl ProofContentsVerifier for OCSProof {
    fn verify(self, target: &ProofTarget, summary: &VerifiedCheckpoint) -> ProofResult<()> {
        match target {
            ProofTarget::ObjectCheckpointState(target) => {
                let actual_artifacts_digest = summary
                    .data()
                    .checkpoint_artifacts_digest()
                    .map_err(|e| ProofError::GeneralError(e.to_string()))?;

                let expected_artifacts_digest =
                    CheckpointArtifactsDigest::from_artifact_digests(vec![
                        self.get_artifact_digest(),
                    ])
                    .map_err(|e| ProofError::GeneralError(e.to_string()))?;

                if expected_artifacts_digest != *actual_artifacts_digest {
                    return Err(ProofError::ArtifactDigestMismatch);
                }
                // MILESTONE: Artifact digest in the proofs is correct w.r.t the summary

                match self {
                    OCSProof::Inclusion(proof) => proof.verify(target),
                    OCSProof::NonInclusion(proof) => proof.verify(target),
                }
                // MILESTONE: Inclusion/Non-inclusion Proof is correct w.r.t the artifact digest
            }
            _ => Err(ProofError::MismatchedTargetAndProofType),
        }
    }
}

impl ProofBuilder for OCSTarget {
    fn construct(self, checkpoint: &CheckpointData) -> ProofResult<Proof> {
        let artifacts = CheckpointArtifacts::from(checkpoint);
        let modified_object_tree = ModifiedObjectTree::new(&artifacts)?;
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
