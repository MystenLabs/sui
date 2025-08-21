// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use sui_light_client::{
    base::{Proof, ProofContentsVerifier, ProofTarget},
    proof::{
        base::{ProofBuilder, ProofContents},
        ocs::ModifiedObjectTree,
    },
};

use sui_types::{
    base_types::ObjectID,
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CheckpointArtifacts, CheckpointCommitment, VerifiedCheckpoint},
};

// Note: Once checkpoint artifacts are live, we can just read an actual checkpoint file.
// Until then, we use the artifacts.chk file (generated on a localnet with artifacts enabled).
const CHECKPOINT_FILE: &str = "test_files/artifacts.chk";

fn load_checkpoint(file_path: &str) -> CheckpointData {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(file_path);

    let mut file = File::open(d).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    bcs::from_bytes(&buffer).unwrap()
}

fn get_all_modified_objects(checkpoint: &CheckpointData) -> ModifiedObjectTree {
    ModifiedObjectTree::new(&CheckpointArtifacts::from(checkpoint))
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
    let verified_summary =
        VerifiedCheckpoint::new_from_verified(checkpoint.checkpoint_summary.clone());
    let all_objects = get_all_modified_objects(&checkpoint);

    let object_id = ObjectID::from_hex_literal("0x7").unwrap();
    let object_ref = all_objects.get_object_state(object_id).unwrap();

    let target = ProofTarget::new_ocs_inclusion(*object_ref);
    let proof = target.construct(&checkpoint).unwrap();

    // Extract the OCSProof from the proof contents (we only test the inner OCSProof)
    if let ProofContents::ObjectCheckpointStateProof(ocs_proof) = proof.proof_contents {
        assert!(ocs_proof.verify(&proof.targets, &verified_summary).is_ok());
    } else {
        panic!("Expected ObjectCheckpointStateProof");
    }
}

#[test]
fn test_get_object_non_inclusion_proof() {
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
    let verified_summary =
        VerifiedCheckpoint::new_from_verified(checkpoint.checkpoint_summary.clone());
    let all_objects = get_all_modified_objects(&checkpoint);

    let obj_test_cases = [
        "0x1",
        "0x456",
        "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
        "0x7", // included object
    ]
    .map(|id| ObjectID::from_hex_literal(id).unwrap());

    for key in obj_test_cases.iter() {
        let target = ProofTarget::new_ocs_non_inclusion(*key);
        let proof = target.construct(&checkpoint);
        if all_objects.is_object_in_checkpoint(*key) {
            // Should fail to get non-inclusion proof for objects that are in the checkpoint
            assert!(
                proof.is_err(),
                "Should not be able to get non-inclusion proof for included object"
            );
        } else {
            let proof = proof.expect("Should be able to get non-inclusion proof");
            // Extract the OCSProof from the proof contents
            if let ProofContents::ObjectCheckpointStateProof(ocs_proof) = proof.proof_contents {
                assert!(ocs_proof.verify(&proof.targets, &verified_summary).is_ok());
            } else {
                panic!("Expected ObjectCheckpointStateProof");
            }
        }
    }
}

#[test]
fn test_modified_object_tree_properties() {
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
    let artifacts = CheckpointArtifacts::from(&checkpoint);
    let object_tree = ModifiedObjectTree::new(&artifacts);

    // Test that object_map and contents have consistent sizes
    assert_eq!(object_tree.object_pos_map.len(), object_tree.contents.len());

    // Test that all objects in contents are also in object_map
    for (i, object_ref) in object_tree.contents.iter().enumerate() {
        assert_eq!(object_tree.object_pos_map.get(&object_ref.0), Some(&i));
    }

    // Test that objects are sorted (as required by non-inclusion proofs)
    for window in object_tree.contents.windows(2) {
        assert!(window[0].0 < window[1].0, "Objects should be sorted by ID");
    }

    println!("Object tree has {} objects", object_tree.contents.len());
    println!("Root hash: {:?}", object_tree.tree.root().bytes());
}

#[test]
fn test_serialization_roundtrip() {
    let checkpoint = load_checkpoint(CHECKPOINT_FILE);
    let all_objects = get_all_modified_objects(&checkpoint);

    // Test inclusion proof serialization
    if let Some(object_ref) = all_objects.contents.first() {
        let target = ProofTarget::new_ocs_inclusion(*object_ref);
        let proof = target.construct(&checkpoint).unwrap();

        // Test JSON serialization
        let serialized = serde_json::to_string(&proof).expect("Should serialize");
        let _deserialized: Proof = serde_json::from_str(&serialized).expect("Should deserialize");

        println!(
            "Inclusion proof JSON serialization successful for object {}",
            object_ref.0
        );

        // Test BCS serialization
        let bcs_serialized = bcs::to_bytes(&proof).expect("Should serialize with BCS");
        let _bcs_deserialized: Proof =
            bcs::from_bytes(&bcs_serialized).expect("Should deserialize with BCS");

        println!(
            "Inclusion proof BCS serialization successful for object {}",
            object_ref.0
        );
    } else {
        panic!("No object state found in the checkpoint");
    }

    // Test non-inclusion proof serialization
    let non_existent_id = ObjectID::from_hex_literal("0x999999").unwrap();
    if !all_objects.is_object_in_checkpoint(non_existent_id) {
        let target = ProofTarget::new_ocs_non_inclusion(non_existent_id);
        let proof = target
            .construct(&checkpoint)
            .expect("Should be able to get non-inclusion proof");

        // Test JSON serialization
        let serialized = serde_json::to_string(&proof).expect("Should serialize");
        let _deserialized: Proof = serde_json::from_str(&serialized).expect("Should deserialize");

        println!(
            "Non-inclusion proof JSON serialization successful for object {}",
            non_existent_id
        );

        // Test BCS serialization
        let bcs_serialized = bcs::to_bytes(&proof).expect("Should serialize with BCS");
        let _bcs_deserialized: Proof =
            bcs::from_bytes(&bcs_serialized).expect("Should deserialize with BCS");

        println!(
            "Non-inclusion proof BCS serialization successful for object {}",
            non_existent_id
        );
    }
}
