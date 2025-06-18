use anyhow::Result;
use fastcrypto::hash::Blake2b256;
use shared_crypto::merkle::{MerkleAuth, MerkleProof, MerkleTree, Node};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use sui_rpc_api::Client as RpcClient;
use sui_types::base_types::ObjectID;
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::messages_checkpoint::{
    CheckpointArtifact, CheckpointArtifacts, CheckpointArtifactsDigest, CheckpointCommitment,
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

#[derive(Debug)]
pub struct CheckpointArtifactsExtended {
    pub contents: CheckpointArtifacts,
    pub object_map: HashMap<ObjectID, usize>,
    pub tree: MerkleTree,
}

#[derive(Debug)]
pub struct ArtifactInclusionProof {
    pub artifact: CheckpointArtifact,
    pub merkle_proof: MerkleProof<Blake2b256>,
    pub leaf_index: usize,
}

impl ArtifactInclusionProof {
    pub fn verify(&self, root: &CheckpointArtifactsDigest) -> bool {
        self.merkle_proof.verify_proof(
            &Node::from(root.digest.into_inner()),
            bcs::to_bytes(&self.artifact).unwrap().as_slice(),
            self.leaf_index,
        )
    }
}

impl CheckpointArtifactsExtended {
    pub fn new(artifacts: CheckpointArtifacts) -> Self {
        let mut object_map = HashMap::new();
        for (i, artifact) in artifacts.contents.iter().enumerate() {
            if let CheckpointArtifact::AccumulatedObjectChange(o) = artifact {
                let ret = object_map.insert(o.id, i);
                if ret.is_some() {
                    panic!("Object ID {} appears more than once", o.id);
                }
            }
        }
        let tree = MerkleTree::<Blake2b256>::build(
            artifacts
                .contents
                .iter()
                .map(|a| bcs::to_bytes(a).unwrap()),
        );
        CheckpointArtifactsExtended {
            contents: artifacts,
            object_map,
            tree,
        }
    }

    pub fn is_object_in_checkpoint(&self, id: ObjectID) -> bool {
        self.object_map.contains_key(&id)
    }

    pub fn get_object_proof(&self, id: ObjectID) -> Result<ArtifactInclusionProof> {
        let index = self
            .object_map
            .get(&id)
            .ok_or(anyhow::anyhow!("Object ID {} not found", id))?;
        Ok(ArtifactInclusionProof {
            artifact: self.contents.contents[*index].clone(),
            merkle_proof: self.tree.get_proof(*index)?,
            leaf_index: *index,
        })
    }

    pub fn digest(&self) -> CheckpointArtifactsDigest {
        CheckpointArtifactsDigest::from(self.tree.root().bytes())
    }
}

impl From<&CheckpointData> for CheckpointArtifactsExtended {
    fn from(checkpoint_data: &CheckpointData) -> Self {
        let effects = checkpoint_data
            .transactions
            .iter()
            .map(|tx| tx.effects.clone())
            .collect::<Vec<_>>();

        let artifacts = CheckpointArtifacts::from(&effects);
        CheckpointArtifactsExtended::new(artifacts)
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

    let artifacts = CheckpointArtifactsExtended::from(&full_checkpoint);
    println!("Artifacts: {:#?}", artifacts.contents);
    println!("Artifacts digest: {:?}", artifacts.contents.digest());

    assert_eq!(CheckpointCommitment::from(artifacts.digest()), commitments[0]);

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

    #[tokio::test]
    async fn test_derive_artifacts() {
        let checkpoint = load_checkpoint(TEST_CHECKPOINT_FILE);
        let artifacts = CheckpointArtifactsExtended::from(&checkpoint);
        let artifacts_digest = artifacts.digest();
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

    #[tokio::test]
    async fn test_get_object_proof() {
        let checkpoint = load_checkpoint(TEST_CHECKPOINT_FILE);
        let artifacts = CheckpointArtifactsExtended::from(&checkpoint);
        let object_id = ObjectID::from_hex_literal("0x7").unwrap();
        if !artifacts.is_object_in_checkpoint(object_id) {
            panic!("Object ID {} not found in checkpoint", object_id);
        }
        let proof = artifacts.get_object_proof(object_id).unwrap();
        println!("Proof: {:?}", proof);
        assert!(proof.verify(&artifacts.digest()));
    }
}
