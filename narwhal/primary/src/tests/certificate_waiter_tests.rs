// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    certificate_waiter::{CertificateWaiter, GC_RESOLUTION},
    common::create_db_stores,
    core::Core,
    header_waiter::HeaderWaiter,
    metrics::PrimaryMetrics,
    synchronizer::Synchronizer,
};
use fastcrypto::{traits::KeyPair, Hash, SignatureService};
use network::P2pNetwork;
use prometheus::Registry;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use test_utils::CommitteeFixture;
use tokio::sync::watch;
use types::{Certificate, PrimaryMessage, ReconfigureNotification, Round};

#[tokio::test]
async fn process_certificate_missing_parents_in_reverse() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    // kept empty
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    // synchronizer to header waiter
    let (tx_sync_headers, rx_sync_headers) = test_utils::test_channel!(1);
    // synchronizer to certificate waiter
    let (tx_sync_certificates, rx_sync_certificates) = test_utils::test_channel!(1);
    // primary messages
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    // header waiter to primary
    let (tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    // certificate waiter to primary
    let (tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    // proposer back to the core
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    // core -> consensus, we store the output of process_certificate here, a small channel limit may backpressure the test into failure
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(100);
    // core -> proposers, byproduct of certificate processing, a small channel limit could backpressure the test into failure
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Signal consensus round
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let network = anemo::Network::bind("localhost:0")
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let gc_depth: Round = 50;

    // Make a headerWaiter
    let _header_waiter_handle = HeaderWaiter::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        certificates_store.clone(),
        payload_store.clone(),
        rx_consensus_round_updates.clone(),
        gc_depth,
        /* sync_retry_delay */ Duration::from_secs(5),
        /* sync_retry_nodes */ 3,
        rx_reconfigure.clone(),
        rx_sync_headers,
        tx_headers_loopback,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Make a certificate waiter
    let _certificate_waiter_handle = CertificateWaiter::spawn(
        committee.clone(),
        certificates_store.clone(),
        rx_consensus_round_updates.clone(),
        gc_depth,
        rx_reconfigure.clone(),
        rx_sync_certificates,
        tx_certificates_loopback,
        metrics.clone(),
    );

    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache,
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ gc_depth,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
    );

    // Generate headers in successive rounds
    let mut current_round: Vec<_> = Certificate::genesis(&committee)
        .into_iter()
        .map(|cert| cert.header)
        .collect();
    let mut headers = vec![];
    let rounds = 5;
    for i in 0..rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| fixture.certificate(&header).digest())
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents);
        headers.extend(current_round.clone());
    }

    // Avoid any sort of missing payload by pre-populating the batch
    for (digest, worker_id) in headers.iter().flat_map(|h| h.payload.iter()) {
        payload_store.write((*digest, *worker_id), 0u8).await;
    }

    // sanity-check
    assert!(headers.len() == fixture.authorities().count() * rounds as usize); // note we don't include genesis

    // the `rev()` below is important, as we want to test anti-topological arrival
    #[allow(clippy::needless_collect)]
    let ids: Vec<_> = headers
        .iter()
        .map(|header| fixture.certificate(header).digest())
        .collect();
    for header in headers.into_iter().rev() {
        tx_primary_messages
            .send(PrimaryMessage::Certificate(fixture.certificate(&header)))
            .await
            .unwrap();
    }

    // we re-evaluate certificates pending after a little while
    tokio::time::sleep(Duration::from_secs(2)).await;
    // Ensure all certificates are now stored
    for id in ids.into_iter().rev() {
        assert!(certificates_store.read(id).unwrap().is_some());
    }
}

#[tokio::test]
async fn process_certificate_check_gc_fires() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let network_key = primary.network_keypair().copy().private().0.to_bytes();
    let name = primary.public_key();
    let signature_service = SignatureService::new(primary.keypair().copy());

    // kept empty
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewEpoch(committee.clone()));
    // synchronizer to header waiter
    let (tx_sync_headers, rx_sync_headers) = test_utils::test_channel!(1);
    // synchronizer to certificate waiter
    let (tx_sync_certificates, rx_sync_certificates) = test_utils::test_channel!(1);
    // primary messages
    let (tx_primary_messages, rx_primary_messages) = test_utils::test_channel!(1);
    // header waiter to primary
    let (tx_headers_loopback, rx_headers_loopback) = test_utils::test_channel!(1);
    // certificate waiter to primary
    let (tx_certificates_loopback, rx_certificates_loopback) = test_utils::test_channel!(1);
    // proposer back to the core
    let (_tx_headers, rx_headers) = test_utils::test_channel!(1);
    // core -> consensus, we store the output of process_certificate here, a small channel limit may backpressure the test into failure
    let (tx_consensus, _rx_consensus) = test_utils::test_channel!(100);
    // core -> proposers, byproduct of certificate processing, a small channel limit could backpressure the test into failure
    let (tx_parents, _rx_parents) = test_utils::test_channel!(100);

    // Signal consensus round
    let (tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee,
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let network = anemo::Network::bind("localhost:0")
        .server_name("narwhal")
        .private_key(network_key)
        .start(anemo::Router::new())
        .unwrap();

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let gc_depth: Round = 50;

    // Make a headerWaiter
    let _header_waiter_handle = HeaderWaiter::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        certificates_store.clone(),
        payload_store.clone(),
        rx_consensus_round_updates.clone(),
        gc_depth,
        /* sync_retry_delay */ Duration::from_secs(5),
        /* sync_retry_nodes */ 3,
        rx_reconfigure.clone(),
        rx_sync_headers,
        tx_headers_loopback,
        metrics.clone(),
        P2pNetwork::new(network.clone()),
    );

    // Make a certificate waiter
    let _certificate_waiter_handle = CertificateWaiter::spawn(
        committee.clone(),
        certificates_store.clone(),
        rx_consensus_round_updates.clone(),
        gc_depth,
        rx_reconfigure.clone(),
        rx_sync_certificates,
        tx_certificates_loopback,
        metrics.clone(),
    );

    // Spawn the core.
    let _core_handle = Core::spawn(
        name.clone(),
        committee.clone(),
        worker_cache.clone(),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        rx_consensus_round_updates,
        /* gc_depth */ gc_depth,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
        P2pNetwork::new(network),
    );

    // Generate headers in successive rounds
    let mut current_round: Vec<_> = Certificate::genesis(&committee)
        .into_iter()
        .map(|cert| cert.header)
        .collect();
    let mut headers = vec![];
    let rounds = 5;
    for i in 0..rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| fixture.certificate(&header).digest())
            .collect();
        (_, current_round) = fixture.headers_round(i, &parents);
        headers.extend(current_round.clone());
    }

    // Avoid any sort of missing payload by pre-populating the batch
    for (digest, worker_id) in headers.iter().flat_map(|h| h.payload.iter()) {
        payload_store.write((*digest, *worker_id), 0u8).await;
    }

    // sanity-check
    assert!(headers.len() == fixture.authorities().count() * rounds as usize); // note we don't include genesis

    // Just send the last header, the causal certificate completion cannot complete
    let header = headers.last().unwrap();
    let cert = fixture.certificate(header);
    let id = cert.digest();

    tx_primary_messages
        .send(PrimaryMessage::Certificate(cert))
        .await
        .unwrap();

    // check the header is still not written (see also process_header_missing_parent)
    assert!(certificates_store.read(id).unwrap().is_none());

    // wait a little bit so we give the change to the message to get processed
    tokio::time::sleep(Duration::from_millis(1_000)).await;

    // Move the round so that this pending certificate moves well past the GC bound
    tx_consensus_round_updates.send(60u64).unwrap();

    // we re-evaluate pending after a little while
    tokio::time::sleep(Duration::from_millis(GC_RESOLUTION)).await;

    // check the header is written, as the cert has been delivered w/o antecedents
    assert!(certificates_store.read(id).unwrap().is_some());
}
