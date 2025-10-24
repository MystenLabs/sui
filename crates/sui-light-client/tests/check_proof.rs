// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;

use sui_light_client::proof::{
    base::{Proof, ProofBuilder, ProofContents, ProofTarget, ProofVerifier},
    committee::{CommitteeProof, extract_new_committee_info},
    objects::ObjectsTarget,
};

use sui_types::event::{Event, EventID};

use sui_types::{committee::Committee, effects::TransactionEffectsAPI, object::Object};

use sui_types::full_checkpoint_content::CheckpointData;

use std::io::Read;
use std::{fs, path::PathBuf};

async fn read_full_checkpoint(checkpoint_path: &PathBuf) -> anyhow::Result<CheckpointData> {
    println!("Reading checkpoint from {:?}", checkpoint_path);
    let mut reader = fs::File::open(checkpoint_path.clone())?;
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    let (_, data): (u8, CheckpointData) =
        bcs::from_bytes(&buffer).map_err(|e| anyhow!("Unable to parse checkpoint file: {}", e))?;
    Ok(data)
}

async fn read_data(committee_seq: u64, seq: u64) -> (Committee, CheckpointData) {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(format!("test_files/{}.chk", committee_seq));

    let committee_checkpoint = read_full_checkpoint(&d).await.unwrap();

    let committee = extract_new_committee_info(&committee_checkpoint.checkpoint_summary).unwrap();

    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(format!("test_files/{}.chk", seq));

    let full_checkpoint = read_full_checkpoint(&d).await.unwrap();

    (committee, full_checkpoint)
}

#[tokio::test]
async fn check_can_read_test_data() {
    let (_committee, full_checkpoint) = read_data(15918264, 16005062).await;
    assert!(
        full_checkpoint
            .checkpoint_summary
            .end_of_epoch_data
            .is_some()
    );
}

#[tokio::test]
async fn test_new_committee() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    // Make a committee object using this
    let new_committee = extract_new_committee_info(&full_checkpoint.checkpoint_summary).unwrap();

    let target = ProofTarget::new_committee(new_committee.clone());
    let committee_proof = target.construct(&full_checkpoint).unwrap();

    assert!(committee_proof.verify(&committee).is_ok());
}

// Fail if the new committee does not match the target of the proof
#[tokio::test]
async fn test_incorrect_new_committee() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    let committee_proof = Proof {
        checkpoint_summary: full_checkpoint.checkpoint_summary.clone(),
        proof_contents: ProofContents::CommitteeProof(CommitteeProof {}),
        targets: ProofTarget::new_committee(committee.clone()), // WRONG
    };

    assert!(committee_proof.verify(&committee).is_err());
}

// Fail if the certificate is incorrect even if no proof targets are given
#[tokio::test]
async fn test_fail_incorrect_cert() {
    let (_committee, full_checkpoint) = read_data(15918264, 16005062).await;

    // Make a committee object using this
    let new_committee = extract_new_committee_info(&full_checkpoint.checkpoint_summary).unwrap();

    let target = ProofTarget::new_committee(new_committee.clone());
    let committee_proof = target.construct(&full_checkpoint).unwrap();

    assert!(committee_proof.verify(&new_committee).is_err());
}

#[tokio::test]
async fn test_object_target_fail_no_data() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    let sample_object: Object = full_checkpoint.transactions[0].output_objects[0].clone();
    let sample_ref = sample_object.compute_object_reference();

    let bad_proof = Proof {
        checkpoint_summary: full_checkpoint.checkpoint_summary.clone(),
        proof_contents: ProofContents::CommitteeProof(CommitteeProof {}), // WRONG
        targets: ProofTarget::Objects(ObjectsTarget {
            objects: vec![(sample_ref, sample_object)],
        }),
    };

    assert!(bad_proof.verify(&committee).is_err());
}

#[tokio::test]
async fn test_object_target_success() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    let sample_object: Object = full_checkpoint.transactions[0].output_objects[0].clone();
    let sample_ref = sample_object.compute_object_reference();

    let target = ProofTarget::Objects(ObjectsTarget {
        objects: vec![(sample_ref, sample_object)],
    });
    let object_proof = target.construct(&full_checkpoint).unwrap();

    assert!(object_proof.verify(&committee).is_ok());
}

#[tokio::test]
async fn test_object_target_fail_wrong_object() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    let sample_object: Object = full_checkpoint.transactions[0].output_objects[0].clone();
    let wrong_object: Object = full_checkpoint.transactions[1].output_objects[1].clone();
    let mut sample_ref = sample_object.compute_object_reference();
    let wrong_ref = wrong_object.compute_object_reference();

    let target = ProofTarget::new_objects(vec![(wrong_ref, sample_object.clone())]); // WRONG
    let object_proof = target.construct(&full_checkpoint).unwrap();
    assert!(object_proof.verify(&committee).is_err());

    // Does not exist
    sample_ref.1 = sample_ref.1.next(); // WRONG

    let target = ProofTarget::new_objects(vec![(sample_ref, sample_object)]);
    let object_proof = target.construct(&full_checkpoint).unwrap();
    assert!(object_proof.verify(&committee).is_err());
}

#[tokio::test]
async fn test_event_target_fail_no_data() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    let sample_event: Event = full_checkpoint.transactions[1]
        .events
        .as_ref()
        .unwrap()
        .data[0]
        .clone();
    let sample_eid = EventID::from((
        *full_checkpoint.transactions[1].effects.transaction_digest(),
        0,
    ));

    let bad_proof = Proof {
        checkpoint_summary: full_checkpoint.checkpoint_summary.clone(),
        proof_contents: ProofContents::CommitteeProof(CommitteeProof {}), // WRONG
        targets: ProofTarget::new_events(vec![(sample_eid, sample_event)]),
    };

    assert!(bad_proof.verify(&committee).is_err());
}

#[tokio::test]
async fn test_event_target_success() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    let sample_event: Event = full_checkpoint.transactions[1]
        .events
        .as_ref()
        .unwrap()
        .data[0]
        .clone();
    let sample_eid = EventID::from((
        *full_checkpoint.transactions[1].effects.transaction_digest(),
        0,
    ));

    let target = ProofTarget::new_events(vec![(sample_eid, sample_event)]);
    let event_proof = target.construct(&full_checkpoint).unwrap();

    assert!(event_proof.verify(&committee).is_ok());
}

#[tokio::test]
async fn test_event_target_fail_bad_event() {
    let (committee, full_checkpoint) = read_data(15918264, 16005062).await;

    let sample_event: Event = full_checkpoint.transactions[1]
        .events
        .as_ref()
        .unwrap()
        .data[0]
        .clone();
    let sample_eid = EventID::from((
        *full_checkpoint.transactions[1].effects.transaction_digest(),
        1, // WRONG
    ));

    let target = ProofTarget::new_events(vec![(sample_eid, sample_event)]);
    let event_proof = target.construct(&full_checkpoint).unwrap();

    assert!(event_proof.verify(&committee).is_err());
}
