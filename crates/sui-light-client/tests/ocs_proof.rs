// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use sui_light_client::{
    ocs::{OCSInclusionProof, OCSNonInclusionProof},
    proof::{
        base::{ProofBuilder, ProofContents, ProofTarget},
        ocs::{ModifiedObjectTree, OCSProof, OCSTarget, OCSTargetType},
    },
};

use sui_types::{
    base_types::ObjectID,
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CheckpointArtifacts, CheckpointCommitment},
};

// Note: Once checkpoint artifacts are live, we can just read an actual checkpoint file.
// Until then, we use the artifacts.chk file. It is generated on a localnet with the checkpoint artifacts enabled.
const CHECKPOINT_FILE: &str = "test_files/artifacts.chk";

fn load_checkpoint(file_path: &str) -> CheckpointData {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(file_path);

    let mut file = File::open(d).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    bcs::from_bytes(&buffer).unwrap()
}

#[test]
fn test_derive_artifacts() {
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
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
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
    let artifacts = CheckpointArtifacts::from(&checkpoint);
    let object_tree = ModifiedObjectTree::new(&artifacts);
    let object_id = ObjectID::from_hex_literal("0x7").unwrap();

    if !object_tree.is_object_in_checkpoint(object_id) {
        panic!("Object ID {} not found in checkpoint", object_id);
    }

    // Test the direct proof from ModifiedObjectTree
    let direct_proof = object_tree.get_inclusion_proof(object_id).unwrap();
    println!("Direct inclusion proof: {:?}", direct_proof);

    // Get the actual object state from the tree to use its digest
    let object_state = &object_tree.contents[*object_tree.object_map.get(&object_id).unwrap()];

    // Test using the light client proof system
    let target = OCSTarget::new(object_id, object_state.digest, OCSTargetType::Inclusion).unwrap();
    let light_client_proof = target.construct(&checkpoint).unwrap();

    // Extract the OCSProof from the proof contents
    if let ProofContents::ObjectCheckpointStateProof(ocs_proof) = light_client_proof.proof_contents
    {
        // Create a mock verified checkpoint for testing
        let artifacts_digest = artifacts.digest().unwrap();

        // Test that the proof verifies correctly
        let target_ref =
            if let ProofTarget::ObjectCheckpointState(target) = &light_client_proof.targets {
                target
            } else {
                panic!("Expected ObjectCheckpointState target");
            };

        // For this test, we'll verify the proof directly using the individual proof methods
        if let OCSProof::Inclusion(inclusion_proof) = ocs_proof {
            assert!(inclusion_proof
                .verify(target_ref, &artifacts_digest)
                .is_ok());
            println!("Light client inclusion proof verified successfully!");
        } else {
            panic!("Expected inclusion proof");
        }
    } else {
        panic!("Expected ObjectCheckpointStateProof");
    }
}

#[test]
fn test_get_object_non_inclusion_proof() {
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
    let artifacts = CheckpointArtifacts::from(&checkpoint);
    let object_tree = ModifiedObjectTree::new(&artifacts);
    let artifacts_digest = artifacts.digest().unwrap();

    let obj_test_cases = [
        "0x1",
        "0x456",
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
    ]
    .map(|id| ObjectID::from_hex_literal(id).unwrap());

    for key in obj_test_cases.iter() {
        if object_tree.is_object_in_checkpoint(*key) {
            // Should fail to get non-inclusion proof for objects that are in the checkpoint
            let proof = object_tree.get_non_inclusion_proof(*key);
            assert!(proof.is_err());
        } else {
            println!("Testing non-inclusion proof for object {}", key);

            // Test the direct proof from ModifiedObjectTree
            let direct_proof = object_tree.get_non_inclusion_proof(*key).unwrap();
            println!("Direct non-inclusion proof for {}: {:?}", key, direct_proof);

            // Test using the light client proof system
            let target = OCSTarget::new(*key, None, OCSTargetType::NonInclusion).unwrap();
            let light_client_proof = target.construct(&checkpoint).unwrap();

            // Extract the OCSProof from the proof contents
            if let ProofContents::ObjectCheckpointStateProof(ocs_proof) =
                light_client_proof.proof_contents
            {
                let target_ref = if let ProofTarget::ObjectCheckpointState(target) =
                    &light_client_proof.targets
                {
                    target
                } else {
                    panic!("Expected ObjectCheckpointState target");
                };

                // Test that the proof verifies correctly
                if let OCSProof::NonInclusion(non_inclusion_proof) = ocs_proof {
                    assert!(non_inclusion_proof
                        .verify(target_ref, &artifacts_digest)
                        .is_ok());
                    println!(
                        "Light client non-inclusion proof verified successfully for {}!",
                        key
                    );
                } else {
                    panic!("Expected non-inclusion proof");
                }
            } else {
                panic!("Expected ObjectCheckpointStateProof");
            }
        }
    }

    // Also test the object that IS in the checkpoint (0x7) - should be included
    let included_object_id = ObjectID::from_hex_literal("0x7").unwrap();
    if object_tree.is_object_in_checkpoint(included_object_id) {
        let proof_result = object_tree.get_non_inclusion_proof(included_object_id);
        assert!(
            proof_result.is_err(),
            "Should not be able to get non-inclusion proof for included object"
        );
    }
}

#[test]
fn test_ocs_target_validation() {
    let object_id = ObjectID::from_hex_literal("0x123").unwrap();

    // Should succeed for inclusion with digest
    let target = OCSTarget::new(
        object_id,
        Some(sui_types::digests::ObjectDigest::random()),
        OCSTargetType::Inclusion,
    );
    assert!(target.is_ok());

    // Should succeed for inclusion without digest
    let target = OCSTarget::new(object_id, None, OCSTargetType::Inclusion);
    assert!(target.is_ok());

    // Should succeed for non-inclusion without digest
    let target = OCSTarget::new(object_id, None, OCSTargetType::NonInclusion);
    assert!(target.is_ok());

    // Should fail for non-inclusion with digest
    let target = OCSTarget::new(
        object_id,
        Some(sui_types::digests::ObjectDigest::random()),
        OCSTargetType::NonInclusion,
    );
    assert!(target.is_err());
}

#[test]
fn test_modified_object_tree_properties() {
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
    let artifacts = CheckpointArtifacts::from(&checkpoint);
    let object_tree = ModifiedObjectTree::new(&artifacts);

    // Test that object_map and contents have consistent sizes
    assert_eq!(object_tree.object_map.len(), object_tree.contents.len());

    // Test that all objects in contents are also in object_map
    for (i, object_state) in object_tree.contents.iter().enumerate() {
        assert_eq!(object_tree.object_map.get(&object_state.id), Some(&i));
    }

    // Test that objects are sorted (as required by non-inclusion proofs)
    for window in object_tree.contents.windows(2) {
        assert!(
            window[0].id < window[1].id,
            "Objects should be sorted by ID"
        );
    }

    println!("Object tree has {} objects", object_tree.contents.len());
    println!("Root hash: {:?}", object_tree.tree.root().bytes());
}

#[test]
fn test_serialization_roundtrip() {
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
    let artifacts = CheckpointArtifacts::from(&checkpoint);
    let object_tree = ModifiedObjectTree::new(&artifacts);

    // Test inclusion proof serialization
    if let Some((&object_id, _)) = object_tree.object_map.iter().next() {
        let inclusion_proof = object_tree.get_inclusion_proof(object_id).unwrap();

        // Test JSON serialization
        let serialized = serde_json::to_string(&inclusion_proof).expect("Should serialize");
        let _deserialized: OCSInclusionProof =
            serde_json::from_str(&serialized).expect("Should deserialize");

        println!(
            "Inclusion proof JSON serialization successful for object {}",
            object_id
        );

        // Test BCS serialization
        let bcs_serialized = bcs::to_bytes(&inclusion_proof).expect("Should serialize with BCS");
        let _bcs_deserialized: OCSInclusionProof =
            bcs::from_bytes(&bcs_serialized).expect("Should deserialize with BCS");

        println!(
            "Inclusion proof BCS serialization successful for object {}",
            object_id
        );
    }

    // Test non-inclusion proof serialization
    let non_existent_id = ObjectID::from_hex_literal("0x999999").unwrap();
    if !object_tree.is_object_in_checkpoint(non_existent_id) {
        let non_inclusion_proof = object_tree
            .get_non_inclusion_proof(non_existent_id)
            .unwrap();

        // Test JSON serialization
        let serialized = serde_json::to_string(&non_inclusion_proof).expect("Should serialize");
        let _deserialized: OCSNonInclusionProof =
            serde_json::from_str(&serialized).expect("Should deserialize");

        println!(
            "Non-inclusion proof JSON serialization successful for object {}",
            non_existent_id
        );

        // Test BCS serialization
        let bcs_serialized =
            bcs::to_bytes(&non_inclusion_proof).expect("Should serialize with BCS");
        let _bcs_deserialized: OCSNonInclusionProof =
            bcs::from_bytes(&bcs_serialized).expect("Should deserialize with BCS");

        println!(
            "Non-inclusion proof BCS serialization successful for object {}",
            non_existent_id
        );
    }
}
