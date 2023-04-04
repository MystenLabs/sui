// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{AuthorityIdentifier, Committee, Stake};
use crypto::{PublicKey, Signature};
use fastcrypto::traits::KeyPair;
use indexmap::IndexMap;
use narwhal_types::{Certificate, Header, HeaderV1, Vote, VoteAPI};
use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use test_utils::{AuthorityFixture, CommitteeFixture};

#[tokio::test]
async fn test_certificate_singers_are_ordered() {
    // GIVEN
    let fixture = CommitteeFixture::builder()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .stake_distribution((1..=4).collect()) // provide some non-uniform stake
        .build();
    let committee: Committee = fixture.committee();

    let authorities = fixture.authorities().collect::<Vec<&AuthorityFixture>>();

    // The authority that creates the Header
    let authority = authorities[0];

    let header = HeaderV1::new(authority.id(), 1, 1, IndexMap::new(), BTreeSet::new()).await;

    // WHEN
    let mut votes: Vec<(AuthorityIdentifier, Signature)> = Vec::new();
    let mut sorted_singers: Vec<PublicKey> = Vec::new();

    // The authorities on position 1, 2, 3 are the ones who would sign
    for authority in &authorities[1..=3] {
        sorted_singers.push(authority.keypair().public().clone());

        let vote = Vote::new_with_signer(
            &Header::V1(header.clone()),
            &authority.id(),
            authority.keypair(),
        );
        votes.push((vote.author(), vote.signature().clone()));
    }

    // Just shuffle to ensure that any underlying sorting will work correctly
    votes.shuffle(&mut OsRng);

    // Create a certificate
    let certificate = Certificate::new_unverified(&committee, Header::V1(header), votes).unwrap();

    let (stake, signers) = certificate.signed_by(&committee);

    // THEN
    assert_eq!(signers.len(), 3);

    // AND authorities public keys are returned in order
    assert_eq!(signers, sorted_singers);

    assert_eq!(stake, 9 as Stake);
}
