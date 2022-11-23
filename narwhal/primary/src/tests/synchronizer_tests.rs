// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{common::create_db_stores, synchronizer::Synchronizer};
use consensus::{dag::Dag, metrics::ConsensusMetrics};
use fastcrypto::{hash::Hash, traits::KeyPair};
use prometheus::Registry;
use std::{
    collections::{BTreeSet, HashMap},
    num::NonZeroUsize,
    sync::Arc,
    time::Duration,
};
use test_utils::{make_optimal_signed_certificates, CommitteeFixture};
use tokio::sync::watch;
use types::{error::DagError, Certificate};

#[tokio::test]
async fn deliver_certificate_using_dag() {
    let fixture = CommitteeFixture::builder().build();
    let name = fixture.authorities().next().unwrap().public_key();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let (_, certificates_store, payload_store) = create_db_stores();
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (_tx_consensus, rx_consensus) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let dag = Arc::new(Dag::new(&committee, rx_consensus, consensus_metrics).1);

    let synchronizer = Synchronizer::new(
        name,
        fixture.committee().into(),
        worker_cache.clone(),
        certificates_store,
        payload_store,
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        Some(dag.clone()),
    );

    // create some certificates in a complete DAG form
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let keys = fixture
        .authorities()
        .map(|a| a.keypair().copy())
        .take(3)
        .collect::<Vec<_>>();
    let (mut certificates, _next_parents) =
        make_optimal_signed_certificates(1..=4, &genesis, &committee, &keys);

    // insert the certificates in the DAG
    for certificate in certificates.clone() {
        dag.insert(certificate).await.unwrap();
    }

    // take the last one (top) and test for parents
    let test_certificate = certificates.pop_back().unwrap();

    // ensure that the certificate parents are found
    let parents_available = synchronizer.check_parents(&test_certificate).await.unwrap();
    assert!(parents_available);
}

#[tokio::test]
async fn deliver_certificate_using_store() {
    let fixture = CommitteeFixture::builder().build();
    let name = fixture.authorities().next().unwrap().public_key();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let (_, certificates_store, payload_store) = create_db_stores();
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    let synchronizer = Synchronizer::new(
        name,
        fixture.committee().into(),
        worker_cache.clone(),
        certificates_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    );

    // create some certificates in a complete DAG form
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let keys = fixture
        .authorities()
        .map(|a| a.keypair().copy())
        .take(3)
        .collect::<Vec<_>>();
    let (mut certificates, _next_parents) =
        make_optimal_signed_certificates(1..=4, &genesis, &committee, &keys);

    // insert the certificates in the DAG
    for certificate in certificates.clone() {
        certificates_store.write(certificate).unwrap();
    }

    // take the last one (top) and test for parents
    let test_certificate = certificates.pop_back().unwrap();

    // ensure that the certificate parents are found
    let parents_available = synchronizer.check_parents(&test_certificate).await.unwrap();
    assert!(parents_available);
}

#[tokio::test]
async fn deliver_certificate_not_found_parents() {
    let fixture = CommitteeFixture::builder().build();
    let name = fixture.authorities().next().unwrap().public_key();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let (_, certificates_store, payload_store) = create_db_stores();
    let (tx_certificate_waiter, mut rx_certificate_waiter) = test_utils::test_channel!(1);
    let (_tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);

    let synchronizer = Synchronizer::new(
        name,
        fixture.committee().into(),
        worker_cache.clone(),
        certificates_store,
        payload_store,
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    );

    // create some certificates in a complete DAG form
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let keys = fixture
        .authorities()
        .map(|a| a.keypair().copy())
        .take(3)
        .collect::<Vec<_>>();
    let (mut certificates, _next_parents) =
        make_optimal_signed_certificates(1..=4, &genesis, &committee, &keys);

    // take the last one (top) and test for parents
    let test_certificate = certificates.pop_back().unwrap();

    // we try to find the certificate's parents
    let parents_available = synchronizer.check_parents(&test_certificate).await.unwrap();

    // and we should fail
    assert!(!parents_available);

    let certificate = rx_certificate_waiter.recv().await.unwrap();

    assert_eq!(certificate, test_certificate);
}

#[tokio::test]
async fn sync_batches_drops_old() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder()
        .randomize_ports(true)
        .committee_size(NonZeroUsize::new(4).unwrap())
        .build();
    let worker_cache = fixture.shared_worker_cache();
    let primary = fixture.authorities().next().unwrap();
    let name = primary.public_key();
    let author = fixture.authorities().nth(2).unwrap();
    let network = test_utils::test_network(primary.network_keypair(), primary.address());

    let (_header_store, certificate_store, payload_store) = create_db_stores();
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(1u64);

    let synchronizer = Arc::new(Synchronizer::new(
        name.clone(),
        fixture.committee().into(),
        worker_cache.clone(),
        certificate_store.clone(),
        payload_store.clone(),
        tx_certificate_waiter,
        rx_consensus_round_updates.clone(),
        None,
    ));

    let mut certificates = HashMap::new();
    for _ in 0..3 {
        let header = author
            .header_builder(&fixture.committee())
            .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 0)
            .build(author.keypair())
            .unwrap();

        let certificate = fixture.certificate(&header);
        let digest = certificate.clone().digest();

        certificates.insert(digest, certificate.clone());
        certificate_store.write(certificate.clone()).unwrap();
        for payload in certificate.header.payload {
            payload_store.async_write(payload, 1).await;
        }
    }
    let test_header = author
        .header_builder(&fixture.committee())
        .round(2)
        .parents(certificates.keys().cloned().collect())
        .with_payload_batch(test_utils::fixture_batch_with_transactions(10), 1)
        .build(author.keypair())
        .unwrap();

    tokio::task::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = tx_consensus_round_updates.send(30);
    });
    match synchronizer
        .sync_batches(&test_header, network.clone(), 10)
        .await
    {
        Err(DagError::TooOld(_, _, _)) => (),
        result => panic!("unexpected result {result:?}"),
    }
}
