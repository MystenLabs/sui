// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{common::create_db_stores, helper::Helper, primary::PrimaryMessage, PayloadToken};
use anemo::{types::PeerInfo, PeerId};
use bincode::Options;
use config::WorkerId;
use fastcrypto::{traits::KeyPair, Hash};
use itertools::Itertools;
use network::P2pNetwork;
use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    time::Duration,
};
use storage::CertificateStore;
use store::{reopen, rocks, rocks::DBMap, Store};
use test_utils::{
    fixture_batch_with_transactions, temp_dir, CommitteeFixture, PrimaryToPrimaryMockServer,
    CERTIFICATES_CF, CERTIFICATE_ID_BY_ROUND_CF, PAYLOAD_CF,
};
use tokio::{sync::watch, time::timeout};
use types::{BatchDigest, Certificate, CertificateDigest, ReconfigureNotification, Round};

#[tokio::test]
async fn test_process_certificates_stream_mode() {
    telemetry_subscribers::init_for_testing();
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();
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
        payload_store.clone(),
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
    let (_, certificate_store, payload_store) = create_db_stores();
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
        payload_store.clone(),
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

#[tokio::test]
async fn test_process_payload_availability_success() {
    // GIVEN
    let (_, certificate_store, payload_store) = create_db_stores();
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
        payload_store.clone(),
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
        if i < 7 {
            // write the certificate
            certificate_store.write(certificate.clone()).unwrap();

            for payload in certificate.header.payload {
                payload_store.write(payload, 1).await;
            }
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

    // WHEN requesting the payload availability for all the certificates
    tx_primaries
        .send(PrimaryMessage::PayloadAvailabilityRequest {
            certificate_ids: certificates.keys().copied().collect(),
            requestor: requestor_name,
        })
        .await
        .expect("Couldn't send message");

    let message = timeout(Duration::from_millis(4_000), handler.recv())
        .await
        .unwrap()
        .unwrap();
    let payload_availability = match message {
        PrimaryMessage::PayloadAvailabilityResponse {
            payload_availability,
            from: _,
        } => payload_availability,
        msg => {
            panic!("Didn't expect message {:?}", msg);
        }
    };

    let result_digests: HashSet<CertificateDigest> = payload_availability
        .iter()
        .map(|(digest, _)| *digest)
        .collect();

    assert_eq!(
        result_digests.len(),
        certificates.len(),
        "Returned unique number of certificates don't match the expected"
    );

    // ensure that we have no payload availability for some
    let availability_map = payload_availability.into_iter().counts_by(|c| c.1);

    for (available, found) in availability_map {
        if available {
            assert_eq!(found, 7, "Expected to have available payloads");
        } else {
            assert_eq!(found, 3, "Expected to have non available payloads");
        }
    }
}

#[tokio::test]
async fn test_process_payload_availability_when_failures() {
    // GIVEN
    // We initialise the test stores manually to allow us
    // inject some wrongly serialised values to cause data store errors.
    let rocksdb = rocks::open_cf(
        temp_dir(),
        None,
        &[CERTIFICATES_CF, CERTIFICATE_ID_BY_ROUND_CF, PAYLOAD_CF],
    )
    .expect("Failed creating database");

    let (certificate_map, certificate_id_by_round_map, payload_map) = reopen!(&rocksdb,
        CERTIFICATES_CF;<CertificateDigest, Certificate>,
        CERTIFICATE_ID_BY_ROUND_CF;<(Round, CertificateDigest), u8>,
        PAYLOAD_CF;<(BatchDigest, WorkerId), PayloadToken>);

    let certificate_store = CertificateStore::new(certificate_map, certificate_id_by_round_map);
    let payload_store: Store<(types::BatchDigest, WorkerId), PayloadToken> =
        Store::new(payload_map);

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
        payload_store.clone(),
        rx_reconfigure,
        rx_primaries,
        P2pNetwork::new(network.clone()),
    );

    // AND some mock certificates
    let mut certificate_ids = Vec::new();
    for _ in 0..10 {
        let header = author
            .header_builder(&committee)
            .with_payload_batch(fixture_batch_with_transactions(10), 0)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);
        let id = certificate.clone().digest();

        // In order to test an error scenario that is coming from the data store,
        // we are going to store for the provided certificate ids some unexpected
        // payload in order to blow up the deserialisation.
        let serialised_key = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding()
            .serialize(&id.borrow())
            .expect("Couldn't serialise key");

        // Just serialise the "false" value
        let dummy_value = bincode::serialize(false.borrow()).expect("Couldn't serialise value");

        rocksdb
            .put_cf(
                &rocksdb
                    .cf_handle(CERTIFICATES_CF)
                    .expect("Couldn't find column family"),
                serialised_key,
                dummy_value,
            )
            .expect("Couldn't insert value");

        certificate_ids.push(id);
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

    // WHEN requesting the payload availability for all the certificates
    tx_primaries
        .send(PrimaryMessage::PayloadAvailabilityRequest {
            certificate_ids,
            requestor: requestor_name,
        })
        .await
        .expect("Couldn't send message");

    let message = timeout(Duration::from_millis(4_000), handler.recv())
        .await
        .unwrap()
        .unwrap();
    let payload_availability = match message {
        PrimaryMessage::PayloadAvailabilityResponse {
            payload_availability,
            from: _,
        } => payload_availability,
        msg => {
            panic!("Didn't expect message {:?}", msg);
        }
    };

    // ensure that we have no payload availability for some
    let availability_map = payload_availability.into_iter().counts_by(|c| c.1);

    for (available, found) in availability_map {
        if available {
            assert_eq!(found, 0, "Didn't expect to have available payloads");
        } else {
            assert_eq!(found, 10, "All payloads should be unavailable");
        }
    }
}
