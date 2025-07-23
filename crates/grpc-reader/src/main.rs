use anyhow::Result;
use fastcrypto::error::FastCryptoResult;
use fastcrypto::hash::Blake2b256;
use fastcrypto::merkle::{MerkleNonInclusionProof, MerkleProof, MerkleTree, Node};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use sui_rpc_api::Client as RpcClient;
use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{
    CheckpointArtifacts, CheckpointArtifactsDigest, CheckpointCommitment, ObjectCheckpointState,
};

pub async fn get_checkpoint_via_grpc(checkpoint_number: u64) -> Result<CheckpointData> {
    let sui_client = RpcClient::new("http://localhost:9000").unwrap();
    let checkpoint = sui_client.get_full_checkpoint(checkpoint_number).await?;
    Ok(checkpoint)
}

pub async fn download_and_save_checkpoint(checkpoint_number: u64, file_path: &str) -> Result<()> {
    let full_checkpoint = get_checkpoint_via_grpc(checkpoint_number).await?;
    let mut file = File::create(file_path).unwrap();
    let bytes = bcs::to_bytes(&full_checkpoint).unwrap();
    file.write_all(&bytes).unwrap();
    Ok(())
}

pub fn load_checkpoint(file_path: &str) -> CheckpointData {
    let mut file = File::open(file_path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    bcs::from_bytes(&buffer).unwrap()
}

// The tree of all objects updated in the checkpoint along with their latest state.
pub struct ModifiedObjectTree {
    pub contents: Vec<ObjectCheckpointState>,
    pub tree: MerkleTree<Blake2b256>,
    pub object_map: HashMap<ObjectID, usize>,
}

#[derive(Debug)]
pub struct ObjectInclusionProof {
    pub object_state: ObjectCheckpointState,
    pub merkle_proof: MerkleProof<Blake2b256>,
    pub leaf_index: usize,
}

impl ObjectInclusionProof {
    pub fn verify(&self, root: &CheckpointArtifactsDigest) -> FastCryptoResult<()> {
        self.merkle_proof.verify_proof_with_unserialized_leaf(
            &Node::from(root.digest.into_inner()),
            &self.object_state,
            self.leaf_index,
        )
    }
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

    pub fn is_object_in_checkpoint(&self, id: ObjectID) -> bool {
        self.object_map.contains_key(&id)
    }

    pub fn get_inclusion_proof(&self, id: ObjectID) -> Result<ObjectInclusionProof> {
        let index = self
            .object_map
            .get(&id)
            .ok_or(anyhow::anyhow!("Object ID {} not found", id))?;
        Ok(ObjectInclusionProof {
            object_state: self.contents[*index].clone(),
            merkle_proof: self.tree.get_proof(*index)?,
            leaf_index: *index,
        })
    }

    pub fn get_non_inclusion_proof(&self, id: ObjectID) -> Result<ObjectNonInclusionProof> {
        if self.is_object_in_checkpoint(id) {
            return Err(anyhow::anyhow!("Object ID {} is in checkpoint", id));
        }
        let target_object_state = ObjectCheckpointState::new(id, None);
        let non_inclusion_proof = self
            .tree
            .compute_non_inclusion_proof(&self.contents, &target_object_state)?;
        Ok(ObjectNonInclusionProof {
            id,
            non_inclusion_proof,
        })
    }
}

#[derive(Debug)]
pub struct ObjectNonInclusionProof {
    pub id: ObjectID,
    pub non_inclusion_proof: MerkleNonInclusionProof<ObjectCheckpointState, Blake2b256>,
}

impl ObjectNonInclusionProof {
    pub fn verify(&self, root: &CheckpointArtifactsDigest) -> FastCryptoResult<()> {
        self.non_inclusion_proof.verify_proof(
            &Node::from(root.digest.into_inner()),
            &ObjectCheckpointState::new(self.id, None),
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let checkpoint_number = 2;
    let test_file = format!("test_files/checkpoint-{}.chk", checkpoint_number);

    if !std::path::Path::new(&test_file).exists() {
        println!(
            "Checkpoint {} not found, fetching from local network",
            checkpoint_number
        );
        download_and_save_checkpoint(checkpoint_number, &test_file).await?;
    }

    println!("Loading checkpoint from file {}", test_file);
    let full_checkpoint = load_checkpoint(&test_file);
    let summary = full_checkpoint.checkpoint_summary.data();
    println!("Summary: {:?}", summary);

    let commitments = &full_checkpoint
        .checkpoint_summary
        .data()
        .checkpoint_commitments;
    println!("Commitments: {:?}", commitments);

    let artifacts = CheckpointArtifacts::from(&full_checkpoint);
    let object_tree = ModifiedObjectTree::new(&artifacts);
    println!("Object states: {:#?}", object_tree.contents);
    println!(
        "Object states digest: {:?}",
        object_tree.tree.root().bytes()
    );

    assert_eq!(
        CheckpointCommitment::from(artifacts.digest().unwrap()),
        commitments[0]
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // use sui_json_rpc_types::CheckpointId;
    // use sui_sdk::SuiClientBuilder;
    // use sui_sdk::rpc_types::Checkpoint;

    const TEST_CHECKPOINT_FILE: &str = "test_files/checkpoint-2.chk";

    // async fn get_checkpoint_via_sdk(checkpoint_number: u64) -> Result<Checkpoint> {
    //     let sui_localnet = SuiClientBuilder::default().build_localnet().await?;
    //     let checkpoint = sui_localnet
    //         .read_api()
    //         .get_checkpoint(CheckpointId::SequenceNumber(checkpoint_number))
    //         .await?;
    //     Ok(checkpoint)
    // }

    // #[tokio::test]
    // async fn test_get_checkpoint_via_grpc() {
    //     let sui_client = RpcClient::new("http://localhost:9000").unwrap();
    //     let checkpoint_number = 1;
    //     let checkpoint = sui_client
    //         .get_checkpoint_summary(checkpoint_number)
    //         .await
    //         .unwrap();
    //     println!(
    //         "Checkpoint artifacts digest: {:?} (cp {})",
    //         checkpoint.data().checkpoint_commitments,
    //         checkpoint_number
    //     );
    //     assert!(checkpoint.data().checkpoint_commitments.len() > 0);
    // }

    // TODO: This test is not working. Look into this if SDK support is needed.
    // #[tokio::test]
    // async fn test_get_checkpoint_via_sdk() {
    //     let checkpoint_number = 1;
    //     let checkpoint = get_checkpoint_via_sdk(checkpoint_number).await.unwrap();
    //     println!("Checkpoint: {:?}", checkpoint);
    //     let commitments = &checkpoint.checkpoint_commitments;
    //     println!("Commitments: {:?}", commitments);
    //     assert!(commitments.len() > 0);
    // }

    #[test]
    fn test_derive_artifacts() {
        let checkpoint = load_checkpoint(TEST_CHECKPOINT_FILE);
        let artifacts = CheckpointArtifacts::from(&checkpoint);
        let artifacts_digest = artifacts.digest().unwrap();
        assert_eq!(
            checkpoint
                .checkpoint_summary
                .data()
                .checkpoint_commitments
                .len(),
            1
        );
        assert_eq!(
            CheckpointCommitment::from(artifacts_digest),
            checkpoint.checkpoint_summary.data().checkpoint_commitments[0]
        );
    }

    #[test]
    fn test_get_object_inclusion_proof() {
        let checkpoint = load_checkpoint(TEST_CHECKPOINT_FILE);
        let artifacts = CheckpointArtifacts::from(&checkpoint);
        let object_tree = ModifiedObjectTree::new(&artifacts);
        let object_id = ObjectID::from_hex_literal("0x7").unwrap();
        if !object_tree.is_object_in_checkpoint(object_id) {
            panic!("Object ID {} not found in checkpoint", object_id);
        }
        let proof = object_tree.get_inclusion_proof(object_id).unwrap();
        println!("Proof: {:?}", proof);
        assert!(proof.verify(&artifacts.digest().unwrap()).is_ok());
    }

    #[test]
    fn test_get_object_non_inclusion_proof() {
        let checkpoint = load_checkpoint(TEST_CHECKPOINT_FILE);
        let artifacts = CheckpointArtifacts::from(&checkpoint);
        let object_tree = ModifiedObjectTree::new(&artifacts);
        let obj_test_cases = [
            "0x1",
            "0x456",
            "0x7",
            "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        ]
        .map(|id| ObjectID::from_hex_literal(id).unwrap());
        for key in obj_test_cases.iter() {
            let proof = object_tree.get_non_inclusion_proof(*key);
            if object_tree.is_object_in_checkpoint(*key) {
                assert!(proof.is_err());
            } else {
                println!("Proof: {:?}", proof);
                let proof = proof.unwrap();
                assert!(proof.verify(&artifacts.digest().unwrap()).is_ok());
            }
        }
    }
}
