// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test file tests the validity of the 'certificates' implementation.

use fastcrypto::{
    traits::{KeyPair, Signer},
    Hash, SignatureService,
};
use test_utils::{
    committee, fixture_batch_with_transactions, fixture_header_builder, keys, keys_with_len,
    pure_committee_from_keys_with_mock_ports, shared_worker_cache,
};
use types::{Certificate, Header, Vote};

fn generate_header() -> Header {
    let batch = fixture_batch_with_transactions(10);
    let proposer = &keys(None).pop().unwrap();

    fixture_header_builder()
        .with_payload_batch(batch, 0)
        .build(proposer)
        .unwrap()
}

#[test]
fn test_empty_certificate_verification() {
    let header = generate_header();
    // You should not be allowed to create a certificate that does not satisfying quorum requirements
    assert!(Certificate::new(&committee(None), header.clone(), Vec::new()).is_err());

    let certificate = Certificate::new_unsigned(&committee(None), header, Vec::new()).unwrap();
    assert!(certificate
        .verify(&committee(None), shared_worker_cache(None))
        .is_err());
}

#[tokio::test]
async fn test_valid_certificate_verification() {
    let header = generate_header();

    let mut signatures = Vec::new();

    // 3 Signers satisfies the 2F + 1 signed stake requirement
    for key in &keys(None)[..3] {
        let mut signature_service = SignatureService::new(key.copy());
        let vote = Vote::new(&header, key.public(), &mut signature_service).await;
        signatures.push((vote.author.clone(), vote.signature.clone()));
    }

    let certificate = Certificate::new(&committee(None), header, signatures).unwrap();

    assert!(certificate
        .verify(&committee(None), shared_worker_cache(None))
        .is_ok());
}

#[tokio::test]
async fn test_certificate_insufficient_signatures() {
    let header = generate_header();

    let mut signatures = Vec::new();

    // 2 Signatures. This is less than 2F + 1 (3).
    for key in &keys(None)[..2] {
        let mut signature_service = SignatureService::new(key.copy());
        let vote = Vote::new(&header, key.public(), &mut signature_service).await;
        signatures.push((vote.author.clone(), vote.signature.clone()));
    }

    assert!(Certificate::new(&committee(None), header.clone(), signatures.clone()).is_err());

    let certificate = Certificate::new_unsigned(&committee(None), header, signatures).unwrap();

    assert!(certificate
        .verify(&committee(None), shared_worker_cache(None))
        .is_err());
}

#[tokio::test]
async fn test_certificate_validly_repeated_public_keys() {
    let header = generate_header();

    let mut signatures = Vec::new();

    // 2 Signatures. This is less than 2F + 1 (3).
    for key in &keys(None)[..3] {
        // We double every (pk, signature) pair - these should be ignored when forming the certificate.
        let mut signature_service = SignatureService::new(key.copy());
        let vote = Vote::new(&header, key.public(), &mut signature_service).await;
        signatures.push((vote.author.clone(), vote.signature.clone()));
        signatures.push((vote.author.clone(), vote.signature.clone()));
    }

    let certificate_res = Certificate::new(&committee(None), header, signatures);
    assert!(certificate_res.is_ok());
    let certificate = certificate_res.unwrap();

    assert!(certificate
        .verify(&committee(None), shared_worker_cache(None))
        .is_ok());
}

#[tokio::test]
async fn test_unknown_signature_in_certificate() {
    let header = generate_header();

    let mut signatures = Vec::new();

    // 2 Signatures. This is less than 2F + 1 (3).
    for key in &keys(None)[..2] {
        // We double every (pk, signature) pair - these should be ignored when forming the certificate.
        let mut signature_service = SignatureService::new(key.copy());
        let vote = Vote::new(&header, key.public(), &mut signature_service).await;
        signatures.push((vote.author.clone(), vote.signature.clone()));
    }

    let malicious_key = keys(Some(2)).pop().unwrap();
    let mut signature_service = SignatureService::new(malicious_key.copy());
    let vote = Vote::new(&header, malicious_key.public(), &mut signature_service).await;
    signatures.push((vote.author.clone(), vote.signature));

    assert!(Certificate::new(&committee(None), header, signatures).is_err());
}

//
// This takes really long to run due to an optimization we didn't make (we should cache the genesis committee).
//
proptest::proptest! {
    #[ignore]
    #[test]
    fn test_certificate_verification(
        committee_size in 4..35_usize
    ) {
        let keys = &keys_with_len(None, committee_size);
        let committee = &pure_committee_from_keys_with_mock_ports(&keys[..]);
        let header = generate_header();

        let mut signatures = Vec::new();

        let quorum_threshold = committee.quorum_threshold() as usize;

        let unsigned_certificate = Certificate::new_unsigned(committee, header.clone(), vec![]).unwrap();

        for key in &keys[..quorum_threshold] {
            let signature = key.sign(unsigned_certificate.digest().as_ref());
            signatures.push((key.public().clone(), signature));
        }

        let certificate = Certificate::new(committee, header, signatures).unwrap();

        assert!(certificate
            .verify(committee, shared_worker_cache(None))
            .is_ok());
    }
}
