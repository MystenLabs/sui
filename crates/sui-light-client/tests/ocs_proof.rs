// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use sui_config::genesis::Genesis;

use sui_light_client::{
    base::{Proof, ProofTarget, ProofVerifier},
    proof::{base::ProofBuilder, ocs::ModifiedObjectTree},
};

use anyhow::anyhow;

use sui_types::{
    base_types::ObjectID,
    committee::Committee,
    full_checkpoint_content::CheckpointData,
    messages_checkpoint::{CheckpointArtifacts, CheckpointCommitment},
};

// Note: Once checkpoint artifacts are live, we can just read an actual checkpoint file.
// Until then, we use the artifacts.chk file (generated on a localnet with artifacts enabled).
const GENESIS_FILE: &str = "test_files/ocs/genesis.blob";
const CHECKPOINT_FILE: &str = "test_files/ocs/1137.chk";

// Returns a checkpoint & its corresponding committee
fn load_test_data() -> Result<(CheckpointData, Committee), anyhow::Error> {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(CHECKPOINT_FILE);

    let mut file = File::open(d).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();
    let checkpoint: CheckpointData = bcs::from_bytes(&buffer).unwrap();

    // Extract committee from genesis
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(GENESIS_FILE);
    let genesis_committee = Genesis::load(&d)
        .map_err(|e| anyhow!(format!("Cannot load Genesis: {e}")))?
        .committee()
        .map_err(|e| anyhow!(format!("Cannot load Genesis: {e}")))?;

    // Sanity check
    checkpoint
        .checkpoint_summary
        .verify_with_contents(&genesis_committee, Some(&checkpoint.checkpoint_contents))
        .map_err(|e| anyhow!(format!("Cannot verify checkpoint: {e}")))?;

    Ok((checkpoint, genesis_committee))
}

fn get_all_modified_objects(
    checkpoint: &CheckpointData,
) -> Result<ModifiedObjectTree, anyhow::Error> {
    let all_objects = ModifiedObjectTree::new(&CheckpointArtifacts::from(checkpoint))?;

    // Ensure there are objects in the checkpoint...
    if all_objects.leaves.is_empty() {
        return Err(anyhow!("No objects in the checkpoint"));
    }

    Ok(all_objects)
}

#[test]
fn test_derive_artifacts() {
    let (checkpoint, _) = load_test_data().unwrap();
    println!(
        "Checkpoint sequence number: {:?}",
        checkpoint.checkpoint_summary.sequence_number()
    );
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
    let (checkpoint, committee) = load_test_data().unwrap();
    let all_objects = get_all_modified_objects(&checkpoint).unwrap();

    let object_id = all_objects.leaves[0].0;
    let object_ref = all_objects.get_object_state(object_id).unwrap();

    let target = ProofTarget::new_ocs_inclusion(*object_ref);
    let proof = target.construct(&checkpoint).unwrap();

    assert!(proof.verify(&committee).is_ok());
}

#[test]
fn test_get_object_non_inclusion_proof() {
    let (checkpoint, committee) = load_test_data().unwrap();
    let all_objects = get_all_modified_objects(&checkpoint).unwrap();

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
            assert!(proof.verify(&committee).is_ok());
        }
    }
}

#[test]
fn test_modified_object_tree_properties() {
    let (checkpoint, _) = load_test_data().unwrap();
    let artifacts = CheckpointArtifacts::from(&checkpoint);
    let object_tree = ModifiedObjectTree::new(&artifacts).unwrap();

    // Test that object_map and contents have consistent sizes
    assert_eq!(object_tree.object_pos_map.len(), object_tree.leaves.len());

    // Test that all objects in contents are also in object_map
    for (i, object_ref) in object_tree.leaves.iter().enumerate() {
        assert_eq!(object_tree.object_pos_map.get(&object_ref.0), Some(&i));
    }

    // Test that objects are sorted (as required by non-inclusion proofs)
    for window in object_tree.leaves.windows(2) {
        assert!(window[0].0 < window[1].0, "Objects should be sorted by ID");
    }

    println!("Object tree has {} objects", object_tree.leaves.len());
}

#[test]
fn test_serialization_roundtrip() {
    let (checkpoint, committee) = load_test_data().unwrap();
    let all_objects = get_all_modified_objects(&checkpoint).unwrap();

    // Test inclusion proof serialization
    let object_ref = all_objects
        .leaves
        .first()
        .expect("No objects in the checkpoint");

    let target = ProofTarget::new_ocs_inclusion(*object_ref);
    let proof = target.construct(&checkpoint).unwrap();

    // Test JSON serialization
    let serialized = serde_json::to_string(&proof).expect("Should serialize");
    let deserialized: Proof = serde_json::from_str(&serialized).expect("Should deserialize");

    assert!(deserialized.verify(&committee).is_ok());

    // Test BCS serialization
    let bcs_serialized = bcs::to_bytes(&proof).expect("Should serialize with BCS");
    let bcs_deserialized: Proof =
        bcs::from_bytes(&bcs_serialized).expect("Should deserialize with BCS");

    assert!(bcs_deserialized.verify(&committee).is_ok());

    // Test non-inclusion proof serialization
    let non_existent_id = ObjectID::from_hex_literal("0x999999").unwrap();
    if !all_objects.is_object_in_checkpoint(non_existent_id) {
        let target = ProofTarget::new_ocs_non_inclusion(non_existent_id);
        let proof = target
            .construct(&checkpoint)
            .expect("Should be able to get non-inclusion proof");

        // Test JSON serialization
        let serialized = serde_json::to_string(&proof).expect("Should serialize");
        let deserialized: Proof = serde_json::from_str(&serialized).expect("Should deserialize");

        assert!(deserialized.verify(&committee).is_ok());

        // Test BCS serialization
        let bcs_serialized = bcs::to_bytes(&proof).expect("Should serialize with BCS");
        let bcs_deserialized: Proof =
            bcs::from_bytes(&bcs_serialized).expect("Should deserialize with BCS");

        assert!(bcs_deserialized.verify(&committee).is_ok());
    }
}
