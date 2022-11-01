// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{common::create_db_stores, helper::Helper, primary::PrimaryMessage};
use anemo::{types::PeerInfo, PeerId};
use fastcrypto::{hash::Hash, traits::KeyPair};
use network::P2pNetwork;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use test_utils::{fixture_batch_with_transactions, CommitteeFixture, PrimaryToPrimaryMockServer};
use tokio::{sync::watch, time::timeout};
use types::{CertificateDigest, ReconfigureNotification};

#[tokio::test]
async fn test_process_certificates_stream_mode() {
    telemetry_subscribers::init_for_testing();
    // GIVEN
    let (_, certificate_store, _payload_store) = create_db_stores();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let author = fixture.authorities().next().unwrap();
    let name = author.public_key();
    let requestor = fixture.authorities().nth(1).unwrap();
    let requestor_name = requestor.public_key();

    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_primaries, rx_primaries) = test_utils::test_channel!(10);

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(author.network_keypair().copy().private().0.to_bytes())
        .start(anemo::Router::new())
        .unwrap();

    let address = committee.primary(&requestor_name).unwrap();
    let address = network::multiaddr_to_address(&address).unwrap();
    let peer_info = PeerInfo {
        peer_id: PeerId(requestor.network_public_key().0.to_bytes()),
        affinity: anemo::types::PeerAffinity::High,
        address: vec![address],
    };
    network.known_peers().insert(peer_info);

    // AND a helper
    let _helper_handle = Helper::spawn(
        name.clone(),
        committee.clone(),
        certificate_store.clone(),
        rx_reconfigure,
        rx_primaries,
        P2pNetwork::new(network.clone()),
    );

    // AND some mock certificates
    let mut certificates = HashMap::new();
    for _ in 0..5 {
        let header = author
            .header_builder(&committee)
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);
        let id = certificate.clone().digest();

        // write the certificate
        certificate_store.write(certificate.clone()).unwrap();

        certificates.insert(id, certificate.clone());
    }

    // AND spin up a mock node
    let address = committee.primary(&requestor_name).unwrap();
    let requestor_key = requestor.network_keypair().copy();
    let (mut handler, _network) = PrimaryToPrimaryMockServer::spawn(requestor_key, address);

    // Wait for connectivity
    let (mut events, mut peers) = network.subscribe();
    // TODO: duplicated code in this file (4 times).
    while peers.len() != 1 {
        let event = events.recv().await.unwrap();
        match event {
            anemo::types::PeerEvent::NewPeer(peer_id) => peers.push(peer_id),
            anemo::types::PeerEvent::LostPeer(_, _) => {
                panic!("we shouldn't see any lost peer events")
            }
        }
    }

    // WHEN requesting the certificates
    tx_primaries
        .send(PrimaryMessage::CertificatesRequest(
            certificates.keys().copied().collect(),
            requestor_name,
        ))
        .await
        .expect("Couldn't send message");

    let mut digests = HashSet::new();
    for _ in 0..certificates.len() {
        let message = timeout(Duration::from_millis(4_000), handler.recv())
            .await
            .unwrap()
            .unwrap();
        let cert = match message {
            PrimaryMessage::Certificate(certificate) => certificate,
            msg => {
                panic!("Didn't expect message {:?}", msg);
            }
        };

        digests.insert(cert.digest());
    }

    assert_eq!(
        digests.len(),
        certificates.len(),
        "Returned unique number of certificates don't match the expected"
    );
}

#[tokio::test]
async fn test_process_certificates_batch_mode() {
    // GIVEN
    let (_, certificate_store, _) = create_db_stores();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let author = fixture.authorities().next().unwrap();
    let name = author.public_key();
    let requestor = fixture.authorities().nth(1).unwrap();
    let requestor_name = requestor.public_key();
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    let (tx_primaries, rx_primaries) = test_utils::test_channel!(10);

    let own_address = network::multiaddr_to_address(&committee.primary(&name).unwrap()).unwrap();
    let network = anemo::Network::bind(own_address)
        .server_name("narwhal")
        .private_key(author.network_keypair().copy().private().0.to_bytes())
        .start(anemo::Router::new())
        .unwrap();

    let address = committee.primary(&requestor_name).unwrap();
    let address = network::multiaddr_to_address(&address).unwrap();
    let peer_info = PeerInfo {
        peer_id: PeerId(requestor.network_public_key().0.to_bytes()),
        affinity: anemo::types::PeerAffinity::High,
        address: vec![address],
    };
    network.known_peers().insert(peer_info);

    // AND a helper
    let _helper_handle = Helper::spawn(
        name.clone(),
        committee.clone(),
        certificate_store.clone(),
        rx_reconfigure,
        rx_primaries,
        P2pNetwork::new(network.clone()),
    );

    // AND some mock certificates
    let mut certificates = HashMap::new();
    let mut missing_certificates = HashSet::new();

    for i in 0..10 {
        let header = author
            .header_builder(&committee)
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);
        let id = certificate.clone().digest();

        certificates.insert(id, certificate.clone());

        // We want to simulate the scenario of both having some certificates
        // found and some non found. Store only the half. The other half
        // should be returned back as non found.
        if i < 5 {
            // write the certificate
            certificate_store.write(certificate.clone()).unwrap();
        } else {
            missing_certificates.insert(id);
        }
    }

    // AND spin up a mock node
    let address = committee.primary(&requestor_name).unwrap();
    let requestor_key = requestor.network_keypair().copy();
    let (mut handler, _network) = PrimaryToPrimaryMockServer::spawn(requestor_key, address);

    // Wait for connectivity
    let (mut events, mut peers) = network.subscribe();
    while peers.len() != 1 {
        let event = events.recv().await.unwrap();
        match event {
            anemo::types::PeerEvent::NewPeer(peer_id) => peers.push(peer_id),
            anemo::types::PeerEvent::LostPeer(_, _) => {
                panic!("we shouldn't see any lost peer events")
            }
        }
    }

    // WHEN requesting the certificates in batch mode
    tx_primaries
        .send(PrimaryMessage::CertificatesBatchRequest {
            certificate_ids: certificates.keys().copied().collect(),
            requestor: requestor_name,
        })
        .await
        .expect("Couldn't send message");

    let message = timeout(Duration::from_millis(4_000), handler.recv())
        .await
        .unwrap()
        .unwrap();
    let result_certificates = match message {
        PrimaryMessage::CertificatesBatchResponse { certificates, .. } => certificates,
        msg => {
            panic!("Didn't expect message {:?}", msg);
        }
    };

    let result_digests: HashSet<CertificateDigest> = result_certificates
        .iter()
        .map(|(digest, _)| *digest)
        .collect();

    assert_eq!(
        result_digests.len(),
        certificates.len(),
        "Returned unique number of certificates don't match the expected"
    );

    // ensure that we have non found certificates
    let non_found_certificates: usize = result_certificates
        .into_iter()
        .filter(|(digest, certificate)| {
            missing_certificates.contains(digest) && certificate.is_none()
        })
        .count();
    assert_eq!(
        non_found_certificates, 5,
        "Expected to have non found certificates"
    );
}
