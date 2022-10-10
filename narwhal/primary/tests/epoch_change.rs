// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use arc_swap::ArcSwap;
use config::{Committee, Parameters};
use fastcrypto::traits::KeyPair;
use futures::future::join_all;
use narwhal_primary as primary;
use network::{CancelOnDropHandler, P2pNetwork, ReliableNetwork};
use node::NodeStorage;
use primary::{NetworkModel, Primary, CHANNEL_CAPACITY};
use prometheus::Registry;
use std::{sync::Arc, time::Duration};
use test_utils::{ensure_test_environment, random_network, temp_dir, CommitteeFixture};
use tokio::sync::watch;
use types::{ReconfigureNotification, WorkerPrimaryMessage};

/// The epoch changes but the stake distribution and network addresses stay the same.
#[tokio::test]
async fn test_simple_epoch_change() {
    let parameters = Parameters {
        header_num_of_batches_threshold: 1, // One batch digest
        ..Parameters::default()
    };
    ensure_test_environment();

    // The configuration of epoch 0.
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee_0 = fixture.committee();
    let worker_cache_0 = fixture.shared_worker_cache();

    // Spawn the committee of epoch 0.
    let mut rx_channels = Vec::new();
    let mut tx_channels = Vec::new();
    for authority in fixture.authorities() {
        let name = authority.public_key();
        let signer = authority.keypair().copy();

        let (tx_new_certificates, rx_new_certificates) =
            test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
        rx_channels.push(rx_new_certificates);
        let (tx_feedback, rx_feedback) =
            test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
        tx_channels.push(tx_feedback.clone());

        let initial_committee = ReconfigureNotification::NewEpoch(committee_0.clone());
        let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

        let store = NodeStorage::reopen(temp_dir());

        Primary::spawn(
            name,
            signer.copy(),
            authority.network_keypair().copy(),
            Arc::new(ArcSwap::from_pointee(committee_0.clone())),
            worker_cache_0.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            store.consensus_store.clone(),
            /* tx_consensus */ tx_new_certificates,
            /* rx_consensus */ rx_feedback,
            /* dag */ None,
            NetworkModel::Asynchronous,
            tx_reconfigure,
            /* tx_committed_certificates */ tx_feedback,
            &Registry::new(),
            None,
        );
    }

    // Run for a while in epoch 0.
    for rx in rx_channels.iter_mut() {
        loop {
            let certificate = rx.recv().await.unwrap();
            assert_eq!(certificate.epoch(), 0);
            if certificate.round() == 10 {
                break;
            }
        }
    }

    let network = anemo::Network::bind("127.0.0.1:0")
        .server_name("narwhal")
        .private_key(
            crypto::NetworkKeyPair::generate(&mut rand::rngs::OsRng)
                .private()
                .0
                .to_bytes(),
        )
        .start(anemo::Router::new())
        .unwrap();

    for authority in committee_0.authorities.values() {
        let address = network::multiaddr_to_address(&authority.primary_address).unwrap();
        let peer_id = anemo::PeerId(authority.network_key.0.to_bytes());

        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }
    let mut network = P2pNetwork::new(network);

    // Move to the next epochs.
    let mut old_committee = committee_0;
    for epoch in 1..=3 {
        // Move to the next epoch.
        let new_committee = Committee {
            epoch,
            ..old_committee.clone()
        };

        // Notify the old committee to change epoch.
        let message = WorkerPrimaryMessage::Reconfigure(ReconfigureNotification::NewEpoch(
            new_committee.clone(),
        ));
        let mut _do_not_drop: Vec<CancelOnDropHandler<_>> = Vec::new();
        for authority in old_committee.authorities.values() {
            _do_not_drop.push(
                network
                    .send(authority.network_key.to_owned(), &message)
                    .await,
            );
        }

        // Run for a while.
        for rx in rx_channels.iter_mut() {
            loop {
                let certificate = rx.recv().await.unwrap();
                if certificate.epoch() == epoch && certificate.round() == 10 {
                    break;
                }
            }
        }

        old_committee = new_committee;
    }
}

#[allow(clippy::mutable_key_type)]
#[tokio::test]
async fn test_partial_committee_change() {
    telemetry_subscribers::init_for_testing();
    ensure_test_environment();
    let parameters = Parameters {
        header_num_of_batches_threshold: 1, // One batch digest
        ..Parameters::default()
    };

    // Make the committee of epoch 0.
    let mut fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee_0 = fixture.committee();
    let worker_cache_0 = fixture.shared_worker_cache();

    // Spawn the committee of epoch 0.
    let mut epoch_0_rx_channels = Vec::new();
    let mut epoch_0_tx_channels = Vec::new();
    for authority in fixture.authorities() {
        let name = authority.public_key();
        let signer = authority.keypair().copy();

        let (tx_new_certificates, rx_new_certificates) =
            test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
        epoch_0_rx_channels.push(rx_new_certificates);
        let (tx_feedback, rx_feedback) =
            test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
        epoch_0_tx_channels.push(tx_feedback.clone());
        let initial_committee = ReconfigureNotification::NewEpoch(committee_0.clone());
        let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

        let store = NodeStorage::reopen(temp_dir());

        Primary::spawn(
            name,
            signer.copy(),
            authority.network_keypair().copy(),
            Arc::new(ArcSwap::from_pointee(committee_0.clone())),
            worker_cache_0.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            store.consensus_store.clone(),
            /* tx_consensus */ tx_new_certificates,
            /* rx_consensus */ rx_feedback,
            /* dag */ None,
            NetworkModel::Asynchronous,
            tx_reconfigure,
            /* tx_committed_certificates */ tx_feedback,
            &Registry::new(),
            None,
        );
    }

    // Run for a while in epoch 0.
    for rx in epoch_0_rx_channels.iter_mut() {
        loop {
            let certificate = rx.recv().await.unwrap();
            assert_eq!(certificate.epoch(), 0);
            if certificate.round() == 10 {
                break;
            }
        }
    }

    // Make the committee of epoch 1.
    fixture.add_authority();
    fixture.bump_epoch();
    let committee_1 = fixture.committee();
    let worker_cache_1 = fixture.shared_worker_cache();

    // Tell the nodes of epoch 0 to transition to epoch 1.
    let network = anemo::Network::bind("127.0.0.1:0")
        .server_name("narwhal")
        .private_key(
            crypto::NetworkKeyPair::generate(&mut rand::rngs::OsRng)
                .private()
                .0
                .to_bytes(),
        )
        .start(anemo::Router::new())
        .unwrap();

    for authority in committee_0.authorities.values() {
        let address = network::multiaddr_to_address(&authority.primary_address).unwrap();
        let peer_id = anemo::PeerId(authority.network_key.0.to_bytes());

        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }
    let mut network = P2pNetwork::new(network);

    let message =
        WorkerPrimaryMessage::Reconfigure(ReconfigureNotification::NewEpoch(committee_1.clone()));
    let mut _do_not_drop: Vec<CancelOnDropHandler<_>> = Vec::new();
    for authority in committee_0.authorities.values() {
        _do_not_drop.push(
            network
                .send(authority.network_key.to_owned(), &message)
                .await,
        );
    }

    // Spawn the committee of epoch 1 (only the node not already booted).
    let mut epoch_1_rx_channels = Vec::new();
    let mut epoch_1_tx_channels = Vec::new();
    if let Some(authority) = fixture.authorities().last() {
        let name = authority.public_key();
        let signer = authority.keypair().copy();

        let (tx_new_certificates, rx_new_certificates) =
            test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
        epoch_1_rx_channels.push(rx_new_certificates);
        let (tx_feedback, rx_feedback) =
            test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
        epoch_1_tx_channels.push(tx_feedback.clone());

        let initial_committee = ReconfigureNotification::NewEpoch(committee_1.clone());
        let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

        let store = NodeStorage::reopen(temp_dir());

        Primary::spawn(
            name,
            signer.copy(),
            authority.network_keypair().copy(),
            Arc::new(ArcSwap::from_pointee(committee_1.clone())),
            worker_cache_1.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store,
            store.vote_digest_store,
            store.consensus_store,
            /* tx_consensus */ tx_new_certificates,
            /* rx_consensus */ rx_feedback,
            /* dag */ None,
            NetworkModel::Asynchronous,
            tx_reconfigure,
            /* tx_committed_certificates */ tx_feedback,
            &Registry::new(),
            None,
        );
    }

    // Run for a while in epoch 1.
    for rx in epoch_1_rx_channels.iter_mut() {
        loop {
            let certificate = rx.recv().await.unwrap();
            if certificate.epoch() == 1 && certificate.round() == 10 {
                break;
            }
        }
    }
}

/// The epoch changes but the stake distribution and network addresses stay the same.
#[tokio::test]
#[ignore]
async fn test_restart_with_new_committee_change() {
    telemetry_subscribers::init_for_testing();
    ensure_test_environment();

    let parameters = Parameters {
        header_num_of_batches_threshold: 1, // One batch digest
        ..Parameters::default()
    };

    // The configuration of epoch 0.
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee_0 = fixture.committee();
    let worker_cache_0 = fixture.shared_worker_cache();

    let network = random_network();

    // Spawn the committee of epoch 0.
    let mut rx_channels = Vec::new();
    let mut tx_channels = Vec::new();
    let mut handles = Vec::new();
    for authority in fixture.authorities() {
        let name = authority.public_key();
        let signer = authority.keypair().copy();

        let (tx_new_certificates, rx_new_certificates) =
            test_utils::test_new_certificates_channel!(CHANNEL_CAPACITY);
        rx_channels.push(rx_new_certificates);
        let (tx_feedback, rx_feedback) =
            test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
        tx_channels.push(tx_feedback.clone());

        let initial_committee = ReconfigureNotification::NewEpoch(committee_0.clone());
        let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

        let store = NodeStorage::reopen(temp_dir());

        let primary_handles = Primary::spawn(
            name,
            signer.copy(),
            authority.network_keypair().copy(),
            Arc::new(ArcSwap::new(Arc::new(committee_0.clone()))),
            worker_cache_0.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            store.consensus_store.clone(),
            /* tx_consensus */ tx_new_certificates,
            /* rx_consensus */ rx_feedback,
            /* dag */ None,
            NetworkModel::Asynchronous,
            tx_reconfigure,
            /* tx_committed_certificates */ tx_feedback,
            &Registry::new(),
            None,
        );
        handles.extend(primary_handles);
    }

    // Run for a while in epoch 0.
    for rx in rx_channels.iter_mut() {
        loop {
            let certificate = rx.recv().await.unwrap();
            assert_eq!(certificate.epoch(), 0);
            if certificate.round() == 10 {
                break;
            }
        }
    }

    // Shutdown the committee of the previous epoch;
    let message = WorkerPrimaryMessage::Reconfigure(ReconfigureNotification::Shutdown);
    let mut _do_not_drop: Vec<CancelOnDropHandler<_>> = Vec::new();
    for authority in committee_0.authorities.values() {
        network
            .connect_with_peer_id(
                network::multiaddr_to_address(&authority.primary_address).unwrap(),
                anemo::PeerId(authority.network_key.0.to_bytes()),
            )
            .await
            .unwrap();
        _do_not_drop.push(
            P2pNetwork::new(network.clone())
                .send(authority.network_key.to_owned(), &message)
                .await,
        );
    }

    // Wait for the committee to shutdown.
    join_all(handles).await;
    // Provide a small amount of time for any background tasks to shutdown
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Move to the next epochs.
    for epoch in 1..=3 {
        tracing::warn!("HERE");
        let mut new_committee = committee_0.clone();
        new_committee.epoch = epoch;
        let old_worker_cache = &mut worker_cache_0.clone().load().clone();
        let mut new_worker_cache = Arc::make_mut(old_worker_cache);
        new_worker_cache.epoch = epoch;

        let mut rx_channels = Vec::new();
        let mut tx_channels = Vec::new();
        let mut handles = Vec::new();
        for authority in fixture.authorities() {
            let name = authority.public_key();
            let signer = authority.keypair().copy();

            let (tx_new_certificates, rx_new_certificates) =
                test_utils::test_channel!(CHANNEL_CAPACITY);
            rx_channels.push(rx_new_certificates);
            let (tx_feedback, rx_feedback) =
                test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
            tx_channels.push(tx_feedback.clone());

            let initial_committee = ReconfigureNotification::NewEpoch(new_committee.clone());
            let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

            let store = NodeStorage::reopen(temp_dir());

            let primary_handles = Primary::spawn(
                name,
                signer.copy(),
                authority.network_keypair().copy(),
                Arc::new(ArcSwap::new(Arc::new(new_committee.clone()))),
                Arc::new(ArcSwap::new(Arc::new(new_worker_cache.clone()))),
                parameters.clone(),
                store.header_store.clone(),
                store.certificate_store.clone(),
                store.proposer_store.clone(),
                store.payload_store.clone(),
                store.vote_digest_store.clone(),
                store.consensus_store.clone(),
                /* tx_consensus */ tx_new_certificates,
                /* rx_consensus */ rx_feedback,
                /* dag */ None,
                NetworkModel::Asynchronous,
                tx_reconfigure,
                /* tx_committed_certificates */ tx_feedback,
                &Registry::new(),
                None,
            );
            handles.extend(primary_handles);
        }

        // Run for a while.
        for rx in rx_channels.iter_mut() {
            loop {
                let certificate = rx.recv().await.unwrap();
                if certificate.epoch() == epoch && certificate.round() == 10 {
                    break;
                }
            }
        }

        // Shutdown the committee of the previous epoch;
        let message = WorkerPrimaryMessage::Reconfigure(ReconfigureNotification::Shutdown);
        let mut _do_not_drop: Vec<CancelOnDropHandler<_>> = Vec::new();
        for authority in committee_0.authorities.values() {
            network
                .connect_with_peer_id(
                    network::multiaddr_to_address(&authority.primary_address).unwrap(),
                    anemo::PeerId(authority.network_key.0.to_bytes()),
                )
                .await
                .unwrap();
            _do_not_drop.push(
                P2pNetwork::new(network.clone())
                    .send(authority.network_key.to_owned(), &message)
                    .await,
            );
        }

        // Wait for the committee to shutdown.
        join_all(handles).await;
        // Provide a small amount of time for any background tasks to shutdown
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Update the committee without changing the epoch.
#[tokio::test]
async fn test_simple_committee_update() {
    ensure_test_environment();
    let parameters = Parameters {
        header_num_of_batches_threshold: 1, // One batch digest
        ..Parameters::default()
    };

    // The configuration of epoch 0.
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee_0 = fixture.committee();
    let worker_cache_0 = fixture.shared_worker_cache();

    // Spawn the committee of epoch 0.
    let mut rx_channels = Vec::new();
    let mut tx_channels = Vec::new();
    for authority in fixture.authorities() {
        let name = authority.public_key();
        let signer = authority.keypair().copy();

        let (tx_new_certificates, rx_new_certificates) =
            test_utils::test_channel!(CHANNEL_CAPACITY);
        rx_channels.push(rx_new_certificates);
        let (tx_feedback, rx_feedback) =
            test_utils::test_committed_certificates_channel!(CHANNEL_CAPACITY);
        tx_channels.push(tx_feedback.clone());

        let initial_committee = ReconfigureNotification::NewEpoch(committee_0.clone());
        let (tx_reconfigure, _rx_reconfigure) = watch::channel(initial_committee);

        let store = NodeStorage::reopen(temp_dir());

        Primary::spawn(
            name,
            signer.copy(),
            authority.network_keypair().copy(),
            Arc::new(ArcSwap::from_pointee(committee_0.clone())),
            worker_cache_0.clone(),
            parameters.clone(),
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            store.consensus_store.clone(),
            /* tx_consensus */ tx_new_certificates,
            /* rx_consensus */ rx_feedback,
            /* dag */ None,
            NetworkModel::Asynchronous,
            tx_reconfigure,
            /* tx_committed_certificates */ tx_feedback,
            &Registry::new(),
            None,
        );
    }

    // Run for a while in epoch 0.
    for rx in rx_channels.iter_mut() {
        loop {
            let certificate = rx.recv().await.unwrap();
            assert_eq!(certificate.epoch(), 0);
            if certificate.round() == 10 {
                break;
            }
        }
    }

    // Update the committee
    let mut old_committee = committee_0;
    for _ in 1..=3 {
        // Update the committee
        let mut new_committee = old_committee.clone();
        for (_, authority) in new_committee.authorities.iter_mut() {
            authority.stake += 1;
        }

        // Notify the old committee about the change in committee information.
        let message = WorkerPrimaryMessage::Reconfigure(ReconfigureNotification::UpdateCommittee(
            new_committee.clone(),
        ));
        let mut _do_not_drop: Vec<CancelOnDropHandler<_>> = Vec::new();
        for authority in old_committee.authorities.values() {
            let mut network = P2pNetwork::new_for_single_address(
                authority.network_key.to_owned(),
                network::multiaddr_to_address(&authority.primary_address).unwrap(),
            )
            .await;
            _do_not_drop.push(
                network
                    .send(authority.network_key.to_owned(), &message)
                    .await,
            );
        }

        // Run for a while.
        for rx in rx_channels.iter_mut() {
            loop {
                let certificate = rx.recv().await.unwrap();
                assert_eq!(certificate.epoch(), 0);
                if certificate.round() == 10 {
                    break;
                }
            }
        }

        old_committee = new_committee;
    }
}
