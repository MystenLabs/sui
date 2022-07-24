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
use core::sync::atomic::AtomicU64;
use crypto::{traits::KeyPair, Hash, SignatureService};
use prometheus::Registry;
use std::{
    collections::BTreeSet,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use test_utils::{certificate, committee, fixture_headers_round, keys};
use tokio::sync::{mpsc::channel, watch};
use types::{Certificate, PrimaryMessage, ReconfigureNotification, Round};

#[tokio::test]
async fn process_certificate_missing_parents_in_reverse() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    // kept empty
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee(None)));
    // synchronizer to header waiter
    let (tx_sync_headers, rx_sync_headers) = channel(1);
    // synchronizer to certificate waiter
    let (tx_sync_certificates, rx_sync_certificates) = channel(1);
    // primary messages
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    // header waiter to primary
    let (tx_headers_loopback, rx_headers_loopback) = channel(1);
    // certificate waiter to primary
    let (tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    // proposer back to the core
    let (_tx_headers, rx_headers) = channel(1);
    // core -> consensus, we store the output of process_certificate here, a small channel limit may backpressure the test into failure
    let (tx_consensus, _rx_consensus) = channel(100);
    // core -> proposers, byproduct of certificate processing, a small channel limit could backpressure the test into failure
    let (tx_parents, _rx_parents) = channel(100);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee(None),
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let consensus_round = Arc::new(AtomicU64::new(0));
    let gc_depth: Round = 50;

    // Make a headerWaiter
    HeaderWaiter::spawn(
        name.clone(),
        committee(None),
        certificates_store.clone(),
        payload_store.clone(),
        consensus_round.clone(),
        gc_depth,
        /* sync_retry_delay */ Duration::from_secs(5),
        /* sync_retry_nodes */ 3,
        rx_reconfigure.clone(),
        rx_sync_headers,
        tx_headers_loopback,
        metrics.clone(),
    );

    // Make a certificate waiter
    CertificateWaiter::spawn(
        committee(None),
        certificates_store.clone(),
        consensus_round.clone(),
        gc_depth,
        rx_reconfigure.clone(),
        rx_sync_certificates,
        tx_certificates_loopback,
        metrics.clone(),
    );

    // Spawn the core.
    Core::spawn(
        name.clone(),
        committee(None),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ consensus_round,
        /* gc_depth */ gc_depth,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
    );

    // Generate headers in successive rounds
    let mut current_round: Vec<_> = Certificate::genesis(&committee(None))
        .into_iter()
        .map(|cert| cert.header)
        .collect();
    let mut headers = vec![];
    let rounds = 5;
    for i in 0..rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| certificate(&header).digest())
            .collect();
        (_, current_round) = fixture_headers_round(i, &parents);
        headers.extend(current_round.clone());
    }

    // Avoid any sort of missing payload by pre-populating the batch
    for (digest, worker_id) in headers.iter().flat_map(|h| h.payload.iter()) {
        payload_store.write((*digest, *worker_id), 0u8).await;
    }

    // sanity-check
    assert!(headers.len() == keys(None).len() * rounds as usize); // note we don't include genesis

    // the `rev()` below is important, as we want to test anti-topological arrival
    #[allow(clippy::needless_collect)]
    let ids: Vec<_> = headers
        .iter()
        .map(|header| certificate(header).digest())
        .collect();
    for header in headers.into_iter().rev() {
        tx_primary_messages
            .send(PrimaryMessage::Certificate(certificate(&header)))
            .await
            .unwrap();
    }

    // we re-evaluate certificates pending after a little while
    tokio::time::sleep(Duration::from_secs(2)).await;
    // Ensure all certificates are now stored
    for id in ids.into_iter().rev() {
        assert!(certificates_store.read(id).await.unwrap().is_some());
    }
}

#[ignore]
#[tokio::test]
async fn process_certificate_check_gc_fires() {
    let kp = keys(None).pop().unwrap();
    let name = kp.public().clone();
    let signature_service = SignatureService::new(kp);

    // kept empty
    let (_tx_reconfigure, rx_reconfigure) =
        watch::channel(ReconfigureNotification::NewCommittee(committee(None)));
    // synchronizer to header waiter
    let (tx_sync_headers, rx_sync_headers) = channel(1);
    // synchronizer to certificate waiter
    let (tx_sync_certificates, rx_sync_certificates) = channel(1);
    // primary messages
    let (tx_primary_messages, rx_primary_messages) = channel(1);
    // header waiter to primary
    let (tx_headers_loopback, rx_headers_loopback) = channel(1);
    // certificate waiter to primary
    let (tx_certificates_loopback, rx_certificates_loopback) = channel(1);
    // proposer back to the core
    let (_tx_headers, rx_headers) = channel(1);
    // core -> consensus, we store the output of process_certificate here, a small channel limit may backpressure the test into failure
    let (tx_consensus, _rx_consensus) = channel(100);
    // core -> proposers, byproduct of certificate processing, a small channel limit could backpressure the test into failure
    let (tx_parents, _rx_parents) = channel(100);

    // Create test stores.
    let (header_store, certificates_store, payload_store) = create_db_stores();

    // Make a synchronizer for the core.
    let synchronizer = Synchronizer::new(
        name.clone(),
        &committee(None),
        certificates_store.clone(),
        payload_store.clone(),
        /* tx_header_waiter */ tx_sync_headers,
        /* tx_certificate_waiter */ tx_sync_certificates,
        None,
    );

    let metrics = Arc::new(PrimaryMetrics::new(&Registry::new()));
    let consensus_round = Arc::new(AtomicU64::new(0));
    let gc_depth: Round = 50;

    // Make a headerWaiter
    HeaderWaiter::spawn(
        name.clone(),
        committee(None),
        certificates_store.clone(),
        payload_store.clone(),
        consensus_round.clone(),
        gc_depth,
        /* sync_retry_delay */ Duration::from_secs(5),
        /* sync_retry_nodes */ 3,
        rx_reconfigure.clone(),
        rx_sync_headers,
        tx_headers_loopback,
        metrics.clone(),
    );

    // Make a certificate waiter
    CertificateWaiter::spawn(
        committee(None),
        certificates_store.clone(),
        consensus_round.clone(),
        gc_depth,
        rx_reconfigure.clone(),
        rx_sync_certificates,
        tx_certificates_loopback,
        metrics.clone(),
    );

    // Spawn the core.
    Core::spawn(
        name.clone(),
        committee(None),
        header_store.clone(),
        certificates_store.clone(),
        synchronizer,
        signature_service,
        /* consensus_round */ consensus_round.clone(),
        /* gc_depth */ gc_depth,
        rx_reconfigure,
        /* rx_primaries */ rx_primary_messages,
        /* rx_header_waiter */ rx_headers_loopback,
        /* rx_certificate_waiter */ rx_certificates_loopback,
        /* rx_proposer */ rx_headers,
        tx_consensus,
        /* tx_proposer */ tx_parents,
        metrics.clone(),
    );

    // Generate headers in successive rounds
    let mut current_round: Vec<_> = Certificate::genesis(&committee(None))
        .into_iter()
        .map(|cert| cert.header)
        .collect();
    let mut headers = vec![];
    let rounds = 5;
    for i in 0..rounds {
        let parents: BTreeSet<_> = current_round
            .into_iter()
            .map(|header| certificate(&header).digest())
            .collect();
        (_, current_round) = fixture_headers_round(i, &parents);
        headers.extend(current_round.clone());
    }

    // Avoid any sort of missing payload by pre-populating the batch
    for (digest, worker_id) in headers.iter().flat_map(|h| h.payload.iter()) {
        payload_store.write((*digest, *worker_id), 0u8).await;
    }

    // sanity-check
    assert!(headers.len() == keys(None).len() * rounds as usize); // note we don't include genesis

    // Just send the last header, the causal certificate completion cannot complete
    let header = headers.last().unwrap();
    let cert = certificate(header);
    let id = cert.digest();

    tx_primary_messages
        .send(PrimaryMessage::Certificate(cert))
        .await
        .unwrap();

    // check the header is still not written (see also process_header_missing_parent)
    assert!(certificates_store.read(id).await.unwrap().is_none());

    // Move the round so that this pending certificate moves well past the GC bound
    assert_eq!(consensus_round.fetch_add(60, Ordering::SeqCst), 0);

    // we re-evaluate pending after a little while
    tokio::time::sleep(Duration::from_millis(GC_RESOLUTION)).await;

    // check the header is written, as the cert has been delivered w/o antecedents
    assert!(certificates_store.read(id).await.unwrap().is_some());
}
