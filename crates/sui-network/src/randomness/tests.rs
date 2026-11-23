// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{randomness::*, utils};
use fastcrypto::{groups::bls12381, serde_helpers::ToFromByteArray};
use fastcrypto_threshold::{mocked_dkg, nodes};
use std::collections::BTreeSet;
use sui_macros::sim_test;
use sui_swarm_config::test_utils::CommitteeFixture;
use sui_types::{
    base_types::ConciseableName,
    committee::Committee,
    crypto::{AuthorityPublicKeyBytes, ToFromBytes},
};
use tracing::Instrument;

type PkG = bls12381::G2Element;
type EncG = bls12381::G2Element;

#[sim_test]
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
            None,
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
            None,
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

#[sim_test]
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
            None,
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

#[sim_test]
async fn test_receive_full_sig() {
    telemetry_subscribers::init_for_testing();
    let committee_fixture = CommitteeFixture::generate(rand::rngs::OsRng, 0, 8);
    let committee = committee_fixture.committee();

    let mut randomness_rxs = Vec::new();
    let mut networks: Vec<anemo::Network> = Vec::new();
    let mut nodes = Vec::new();
    let mut handles = Vec::new();
    let mut authority_info = HashMap::new();

    for (i, (authority, stake)) in committee.members().enumerate() {
        let (tx, rx) = mpsc::channel(3);
        randomness_rxs.push(rx);
        let (unstarted, router) = Builder::new(*authority, tx).build();

        let network = utils::build_network(|r| r.merge(router));
        if i < 7 {
            // Leave last network disconnected to test full sig distribution.
            for n in networks.iter() {
                network.connect(n.local_addr()).await.unwrap();
            }
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

    // Authorities 0-6 should be able to complete the sig as normal
    // using partial sigs.
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
            None,
        );
    }
    for rx in randomness_rxs[..7].iter_mut() {
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(0, epoch);
        assert_eq!(0, round.0);
        assert_ne!(0, bytes.len());
    }
    // Authority 7 shouldn't have the completed sig, since it's disconnected.
    assert!(randomness_rxs[7].try_recv().is_err());

    // Connect authority 7 to a single other authority, which should be able to transmit the
    // full sig.
    networks[7].connect(networks[0].local_addr()).await.unwrap();
    let (epoch, round, bytes) = randomness_rxs[7].recv().await.unwrap();
    assert_eq!(0, epoch);
    assert_eq!(0, round.0);
    assert_ne!(0, bytes.len());
}

#[sim_test]
async fn test_restart_recovery() {
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
        handle.send_partial_signatures(0, RandomnessRound(1_000_000));
        handle.update_epoch(
            0,
            authority_info.clone(),
            mock_dkg_output,
            committee.validity_threshold().try_into().unwrap(),
            Some(RandomnessRound(999_999)),
        );
    }
    for rx in randomness_rxs.iter_mut() {
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(0, epoch);
        assert_eq!(1_000_000, round.0);
        assert_ne!(0, bytes.len());
    }
}

#[sim_test]
async fn test_byzantine_peer_handling() {
    telemetry_subscribers::init_for_testing();
    let committee_fixture = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let committee = committee_fixture.committee();

    let mut randomness_rxs = Vec::new();
    let mut networks: Vec<anemo::Network> = Vec::new();
    let mut nodes = Vec::new();
    let mut handles = Vec::new();
    let mut authority_info = HashMap::new();

    for (authority, stake) in committee.members() {
        let config = RandomnessConfig {
            max_ignored_peer_weight_factor: Some(0.3),
            ..Default::default()
        };

        let (tx, rx) = mpsc::channel(3);
        randomness_rxs.push(rx);
        let (unstarted, router) = Builder::new(*authority, tx).config(config).build();

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

    // Send partial sigs from authorities 0 and 1, but give them different DKG output so they think
    // each other are byzantine.
    for (i, (authority, handle)) in handles.iter().enumerate() {
        let mock_dkg_output = mocked_dkg::generate_mocked_output::<PkG, EncG>(
            nodes.clone(),
            committee.validity_threshold().try_into().unwrap(),
            if i < 2 { 100 + i as u128 } else { 0 },
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
            None,
        );
    }
    for rx in &mut randomness_rxs[2..] {
        // Validators (2, 3) can communicate normally.
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(0, epoch);
        assert_eq!(0, round.0);
        assert_ne!(0, bytes.len());
    }
    for rx in &mut randomness_rxs[..2] {
        // Validators (0, 1) are byzantine.
        assert!(rx.try_recv().is_err());
    }

    // New epoch, they should forgive each other.
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
        handle.send_partial_signatures(1, RandomnessRound(0));
        handle.update_epoch(
            1,
            authority_info.clone(),
            mock_dkg_output,
            committee.validity_threshold().try_into().unwrap(),
            None,
        );
    }
    for rx in &mut randomness_rxs[..2] {
        // Validators (0, 1) can communicate normally in new epoch.
        let (epoch, round, bytes) = rx.recv().await.unwrap();
        assert_eq!(1, epoch);
        assert_eq!(0, round.0);
        assert_ne!(0, bytes.len());
    }
    for rx in &mut randomness_rxs[2..] {
        // Validators (2, 3) are still on old epoch.
        assert!(rx.try_recv().is_err());
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
    fastcrypto_threshold::nodes::Node::<EncG> {
        id,
        pk: fastcrypto_threshold::ecies_v1::PublicKey::from(pk),
        weight: stake.try_into().unwrap(),
    }
}

#[sim_test]
async fn test_bundle_key_derivation() {
    telemetry_subscribers::init_for_testing();
    let committee_fixture = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let committee = committee_fixture.committee();
    let threshold: u16 = committee.validity_threshold().try_into().unwrap();

    let mut networks: Vec<anemo::Network> = Vec::new();
    let mut nodes = Vec::new();
    let mut handles = Vec::new();
    let mut authority_info = HashMap::new();
    for (authority, stake) in committee.members() {
        let (tx, _rx) = mpsc::channel(3);
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
        tokio::spawn(r.start());
    }
    let nodes = nodes::Nodes::new(nodes).unwrap();

    let mut public_key = None;
    for (authority, handle) in handles.iter() {
        let mock_dkg_output = mocked_dkg::generate_mocked_output::<PkG, EncG>(
            nodes.clone(),
            threshold.into(),
            0,
            committee
                .authority_index(authority)
                .unwrap()
                .try_into()
                .unwrap(),
        );
        public_key = Some(mock_dkg_output.vss_pk.c0());
        handle.update_epoch(0, authority_info.clone(), mock_dkg_output, threshold, None);
    }
    let public_key = public_key.unwrap();

    let id_message = [
        sui_types::encrypted_bundle::ID_TAG,
        b"test identity".as_slice(),
    ]
    .concat();

    // Only one node requests the key. Its shares alone are below the threshold, and the other
    // nodes do not release theirs, so no key is derived.
    handles[0].1.request_bundle_key(0, id_message.clone());
    let waiter = handles[0].1.wait_bundle_key(0, id_message.clone());
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mut waiter = waiter;
    assert!(waiter.try_recv().is_err());

    // Once every node processed the commit, all nodes derive the same verified key.
    for (_, handle) in handles.iter().skip(1) {
        handle.request_bundle_key(0, id_message.clone());
    }
    let mut keys = vec![waiter.await.unwrap()];
    for (_, handle) in handles.iter().skip(1) {
        keys.push(handle.wait_bundle_key(0, id_message.clone()).await.unwrap());
    }
    for key in &keys {
        assert_eq!(key, &keys[0]);
        ThresholdBls12381MinSig::verify(&public_key, &id_message, key).unwrap();
    }

    // A malformed identity is refused, so the beacon's round messages cannot be signed.
    let round_message = RandomnessRound(5).signature_message();
    for (_, handle) in handles.iter() {
        handle.request_bundle_key(0, round_message.clone());
    }
    let mut waiter = handles[0].1.wait_bundle_key(0, round_message);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(waiter.try_recv().is_err());
}
