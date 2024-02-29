// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{randomness::*, utils};
use fastcrypto::{groups::bls12381, serde_helpers::ToFromByteArray};
use fastcrypto_tbls::{mocked_dkg, nodes};
use sui_swarm_config::test_utils::CommitteeFixture;
use sui_types::{
    base_types::ConciseableName,
    committee::Committee,
    crypto::{AuthorityPublicKeyBytes, ToFromBytes},
};
use tracing::Instrument;

type PkG = bls12381::G2Element;
type EncG = bls12381::G2Element;

#[tokio::test]
async fn test_multiple_epochs() {
    telemetry_subscribers::init_for_testing();
    let committee_fixture = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let committee = committee_fixture.committee();

    let mut randomness_rxs = Vec::new();
    let mut networks: Vec<anemo::Network> = Vec::new();
    let mut nodes = Vec::new();
    let mut handles = Vec::new();
    let mut authority_info = HashMap::new();

    for (authority, stake) in committee.members() {
        let (tx, rx) = mpsc::channel(3);
        randomness_rxs.push(rx);
        let (unstarted, router) = Builder::new(*authority, tx).build();

        let network = utils::build_network(|r| r.merge(router));
        for n in networks.iter() {
            network.connect(n.local_addr()).await.unwrap();
        }
        networks.push(network.clone());

        let node = node_from_committee(committee, authority, *stake);
        authority_info.insert(*authority, (network.peer_id(), node.id));
        nodes.push(node);

        let (r, handle) = unstarted.build(network);
        handles.push((authority, handle));

        let span = tracing::span!(
            tracing::Level::INFO,
            "RandomnessEventLoop",
            authority = ?authority.concise(),
        );
        tokio::spawn(r.start().instrument(span));
    }
    info!(?authority_info, "authorities constructed");

    let nodes = nodes::Nodes::new(nodes).unwrap();

    // Test first round.
    for (authority, handle) in handles.iter() {
        let mock_dkg_output = mocked_dkg::generate_mocked_output::<PkG, EncG>(
            nodes.clone(),
            committee.validity_threshold().try_into().unwrap(),
            0,
            committee
                .authority_index(authority)
                .unwrap()
                .try_into()
                .unwrap(),
        );
        handle.send_partial_signatures(0, RandomnessRound(0));
        handle.update_epoch(
            0,
            authority_info.clone(),
            mock_dkg_output,
            committee.validity_threshold().try_into().unwrap(),
        );
    }
    for rx in randomness_rxs.iter_mut() {
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(0, epoch);
        assert_eq!(0, round.0);
        assert_ne!(0, bytes.len());
    }

    // Test a few more rounds. Generation of rounds in epoch 1 should block until
    // epoch is updated.
    for (_authority, handle) in handles.iter() {
        handle.complete_round(0, RandomnessRound(0));
        handle.send_partial_signatures(0, RandomnessRound(1));
        handle.send_partial_signatures(1, RandomnessRound(0));
        handle.send_partial_signatures(1, RandomnessRound(1));
    }
    for rx in randomness_rxs.iter_mut() {
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(0, epoch);
        assert_eq!(1, round.0);
        assert_ne!(0, bytes.len());
        assert!(rx.try_recv().is_err()); // there should not be anything else ready yet
    }
    for (authority, handle) in handles.iter() {
        // update to epoch 1
        let mock_dkg_output = mocked_dkg::generate_mocked_output::<PkG, EncG>(
            nodes.clone(),
            committee.validity_threshold().try_into().unwrap(),
            1,
            committee
                .authority_index(authority)
                .unwrap()
                .try_into()
                .unwrap(),
        );
        handle.update_epoch(
            1,
            authority_info.clone(),
            mock_dkg_output,
            committee.validity_threshold().try_into().unwrap(),
        );
    }
    let mut rounds_seen = BTreeSet::new(); // use a set because rounds could be generated out-of-order
    for rx in randomness_rxs.iter_mut() {
        // now we expect the two rounds we started earlier to be generated
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(1, epoch);
        rounds_seen.insert(round);
        assert_ne!(0, bytes.len());
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(1, epoch);
        rounds_seen.insert(round);
        assert_ne!(0, bytes.len());
    }
    assert!(rounds_seen.contains(&RandomnessRound(0)));
    assert!(rounds_seen.contains(&RandomnessRound(1)));
}

#[tokio::test]
async fn test_record_own_partial_sigs() {
    telemetry_subscribers::init_for_testing();
    let committee_fixture = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let committee = committee_fixture.committee();

    let mut randomness_rxs = Vec::new();
    let mut networks: Vec<anemo::Network> = Vec::new();
    let mut nodes = Vec::new();
    let mut handles = Vec::new();
    let mut authority_info = HashMap::new();

    for (authority, stake) in committee.members() {
        let (tx, rx) = mpsc::channel(3);
        randomness_rxs.push(rx);
        let (unstarted, router) = Builder::new(*authority, tx).build();

        let network = utils::build_network(|r| r.merge(router));
        for n in networks.iter() {
            network.connect(n.local_addr()).await.unwrap();
        }
        networks.push(network.clone());

        let node = node_from_committee(committee, authority, *stake);
        authority_info.insert(*authority, (network.peer_id(), node.id));
        nodes.push(node);

        let (r, handle) = unstarted.build(network);
        handles.push((authority, handle));

        let span = tracing::span!(
            tracing::Level::INFO,
            "RandomnessEventLoop",
            authority = ?authority.concise(),
        );
        tokio::spawn(r.start().instrument(span));
    }
    info!(?authority_info, "authorities constructed");

    let nodes = nodes::Nodes::new(nodes).unwrap();

    // Only send partial sigs from authorities 0 and 1. They should still be able to reach
    // the threshold to generate full signatures, only if they are correctly recording and using
    // their own partial signatures as well.
    for (authority, handle) in handles.iter().take(2) {
        let mock_dkg_output = mocked_dkg::generate_mocked_output::<PkG, EncG>(
            nodes.clone(),
            committee.validity_threshold().try_into().unwrap(),
            0,
            committee
                .authority_index(authority)
                .unwrap()
                .try_into()
                .unwrap(),
        );
        handle.send_partial_signatures(0, RandomnessRound(0));
        handle.update_epoch(
            0,
            authority_info.clone(),
            mock_dkg_output,
            committee.validity_threshold().try_into().unwrap(),
        );
    }
    for (i, rx) in randomness_rxs.iter_mut().enumerate() {
        if i < 2 {
            let (epoch, round, bytes) = rx.recv().await.unwrap();
            assert_eq!(0, epoch);
            assert_eq!(0, round.0);
            assert_ne!(0, bytes.len());
        } else {
            assert!(rx.try_recv().is_err());
        }
    }
}

fn node_from_committee(
    committee: &Committee,
    authority: &AuthorityPublicKeyBytes,
    stake: u64,
) -> nodes::Node<EncG> {
    let id = committee
        .authority_index(authority)
        .unwrap()
        .try_into()
        .unwrap();
    let pk = bls12381::G2Element::from_byte_array(
        committee
            .public_key(authority)
            .expect("lookup of known committee member should succeed")
            .as_bytes()
            .try_into()
            .expect("key length should match"),
    )
    .expect("should work to convert BLS key to G2Element");
    fastcrypto_tbls::nodes::Node::<EncG> {
        id,
        pk: fastcrypto_tbls::ecies::PublicKey::from(pk),
        weight: stake.try_into().unwrap(),
    }
}
