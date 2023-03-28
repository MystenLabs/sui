// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test file tests the validity of the 'certificates' implementation.

use config::AuthorityIdentifier;
use crypto::KeyPair;
use fastcrypto::traits::KeyPair as _;
use rand::{
    rngs::{OsRng, StdRng},
    SeedableRng,
};
use std::num::NonZeroUsize;
use test_utils::CommitteeFixture;
use types::{Certificate, Vote, VoteAPI};

#[test]
fn test_empty_certificate_verification() {
    let fixture = CommitteeFixture::builder().build();

    let committee = fixture.committee();
    let header = fixture.header();
    // You should not be allowed to create a certificate that does not satisfying quorum requirements
    assert!(Certificate::new_unverified(&committee, header.clone(), Vec::new()).is_err());

    let certificate = Certificate::new_unsigned(&committee, header, Vec::new()).unwrap();
    assert!(certificate
        .verify(&committee, &fixture.worker_cache())
        .is_err());
}

#[test]
fn test_valid_certificate_verification() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let header = fixture.header();

    let mut signatures = Vec::new();

    // 3 Signers satisfies the 2F + 1 signed stake requirement
    for authority in fixture.authorities().take(3) {
        let vote = authority.vote(&header);
        signatures.push((vote.author(), vote.signature().clone()));
    }

    let certificate = Certificate::new_unverified(&committee, header, signatures).unwrap();

    assert!(certificate
        .verify(&committee, &fixture.worker_cache())
        .is_ok());
}

#[test]
fn test_certificate_insufficient_signatures() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let header = fixture.header();

    let mut signatures = Vec::new();

    // 2 Signatures. This is less than 2F + 1 (3).
    for authority in fixture.authorities().take(2) {
        let vote = authority.vote(&header);
        signatures.push((vote.author(), vote.signature().clone()));
    }

    assert!(Certificate::new_unverified(&committee, header.clone(), signatures.clone()).is_err());

    let certificate = Certificate::new_unsigned(&committee, header, signatures).unwrap();

    assert!(certificate
        .verify(&committee, &fixture.worker_cache())
        .is_err());
}

#[test]
fn test_certificate_validly_repeated_public_keys() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let header = fixture.header();

    let mut signatures = Vec::new();

    // 3 Signers satisfies the 2F + 1 signed stake requirement
    for authority in fixture.authorities().take(3) {
        let vote = authority.vote(&header);
        // We double every (pk, signature) pair - these should be ignored when forming the certificate.
        signatures.push((vote.author(), vote.signature().clone()));
        signatures.push((vote.author(), vote.signature().clone()));
    }

    let certificate_res = Certificate::new_unverified(&committee, header, signatures);
    assert!(certificate_res.is_ok());
    let certificate = certificate_res.unwrap();

    assert!(certificate
        .verify(&committee, &fixture.worker_cache())
        .is_ok());
}

#[test]
fn test_unknown_signature_in_certificate() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let header = fixture.header();

    let mut signatures = Vec::new();

    // 2 Signatures. This is less than 2F + 1 (3).
    for authority in fixture.authorities().take(2) {
        let vote = authority.vote(&header);
        signatures.push((vote.author(), vote.signature().clone()));
    }

    let malicious_key = KeyPair::generate(&mut StdRng::from_rng(OsRng).unwrap());
    let malicious_id: AuthorityIdentifier = AuthorityIdentifier(50u16);

    let vote = Vote::new_with_signer(&header, &malicious_id, &malicious_key);
    signatures.push((vote.author(), vote.signature().clone()));

    assert!(Certificate::new_unverified(&committee, header, signatures).is_err());
}

proptest::proptest! {
    #[test]
    fn test_certificate_verification(
        committee_size in 4..35_usize
    ) {
        let fixture = CommitteeFixture::builder()
            .committee_size(NonZeroUsize::new(committee_size).unwrap())
            .build();
        let committee = fixture.committee();
        let header = fixture.header();

        let mut signatures = Vec::new();

        let quorum_threshold = committee.quorum_threshold() as usize;

        for authority in fixture.authorities().take(quorum_threshold) {
            let vote = authority.vote(&header);
            signatures.push((vote.author(), vote.signature().clone()));
        }

        let certificate = Certificate::new_unverified(&committee, header, signatures).unwrap();

        assert!(certificate
            .verify(&committee, &fixture.worker_cache())
            .is_ok());
    }
}
