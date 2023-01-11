// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::mutable_key_type)]

use arc_swap::ArcSwap;
use config::Parameters;
use fastcrypto::traits::KeyPair;
use futures::future::join_all;
use narwhal_primary as primary;
use narwhal_primary::NUM_SHUTDOWN_RECEIVERS;
use primary::{NetworkModel, Primary, CHANNEL_CAPACITY};
use prometheus::Registry;
use std::{collections::HashMap, sync::Arc, time::Duration};
use storage::NodeStorage;
use test_utils::{ensure_test_environment, temp_dir, CommitteeFixture};
use tokio::sync::watch;
use types::{PreSubscribedBroadcastSender, ReconfigureNotification};

/// The epoch changes but the stake distribution and network addresses stay the same.
#[tokio::test]
#[ignore]
async fn test_restart_with_new_committee_change() {
    telemetry_subscribers::init_for_testing();
    ensure_test_environment();

    // The configuration of epoch 0.
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee_0 = fixture.committee();
    let worker_cache_0 = fixture.shared_worker_cache();
    let parameters = fixture
        .authorities()
        .map(|a| {
            (
                a.public_key(),
                Parameters {
                    header_num_of_batches_threshold: 1, // One batch digest
                    ..Parameters::default()
                },
            )
        })
        .collect::<HashMap<_, _>>();

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
        let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0);

        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

        let store = NodeStorage::reopen(temp_dir());

        let p = parameters.get(&name).unwrap().clone();
        let primary_handles = Primary::spawn(
            name,
            signer.copy(),
            authority.network_keypair().copy(),
            Arc::new(ArcSwap::new(Arc::new(committee_0.clone()))),
            worker_cache_0.clone(),
            p,
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            tx_new_certificates,
            rx_feedback,
            rx_consensus_round_updates,
            /* dag */ None,
            NetworkModel::Asynchronous,
            &mut tx_shutdown,
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
    let message = ReconfigureNotification::Shutdown;
    let client = reqwest::Client::new();
    for authority in committee_0.authorities.keys() {
        client
            .post(format!(
                "http://127.0.0.1:{}/reconfigure",
                parameters
                    .get(authority)
                    .unwrap()
                    .network_admin_server
                    .primary_network_admin_server_port
            ))
            .json(&message)
            .send()
            .await
            .unwrap();
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
            let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0);

            let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

            let store = NodeStorage::reopen(temp_dir());

            let p = parameters.get(&name).unwrap().clone();
            let primary_handles = Primary::spawn(
                name,
                signer.copy(),
                authority.network_keypair().copy(),
                Arc::new(ArcSwap::new(Arc::new(new_committee.clone()))),
                Arc::new(ArcSwap::new(Arc::new(new_worker_cache.clone()))),
                p,
                store.header_store.clone(),
                store.certificate_store.clone(),
                store.proposer_store.clone(),
                store.payload_store.clone(),
                store.vote_digest_store.clone(),
                tx_new_certificates,
                rx_feedback,
                rx_consensus_round_updates,
                /* dag */ None,
                NetworkModel::Asynchronous,
                &mut tx_shutdown,
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
        let message = ReconfigureNotification::Shutdown;
        let client = reqwest::Client::new();
        for authority in committee_0.authorities.keys() {
            client
                .post(format!(
                    "http://127.0.0.1:{}/reconfigure",
                    parameters
                        .get(authority)
                        .unwrap()
                        .network_admin_server
                        .primary_network_admin_server_port
                ))
                .json(&message)
                .send()
                .await
                .unwrap();
        }

        // Wait for the committee to shutdown.
        join_all(handles).await;
        // Provide a small amount of time for any background tasks to shutdown
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
