// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{common::create_db_stores, synchronizer::Synchronizer};
use consensus::{dag::Dag, metrics::ConsensusMetrics};
use fastcrypto::{hash::Hash, traits::KeyPair};
use prometheus::Registry;
use std::{collections::BTreeSet, sync::Arc};
use test_utils::{make_optimal_signed_certificates, CommitteeFixture};
use types::Certificate;

#[tokio::test]
async fn deliver_certificate_using_dag() {
    let fixture = CommitteeFixture::builder().build();
    let name = fixture.authorities().next().unwrap().public_key();

    // Make the current committee.
    let committee = fixture.committee();

    let (_, certificates_store, payload_store) = create_db_stores();
    let (tx_header_waiter, _rx_header_waiter) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);
    let (_tx_consensus, rx_consensus) = test_utils::test_channel!(1);

    let consensus_metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
    let dag = Arc::new(Dag::new(&committee, rx_consensus, consensus_metrics).1);

    let mut synchronizer = Synchronizer::new(
        name,
        &committee,
        certificates_store,
        payload_store,
        tx_header_waiter,
        tx_certificate_waiter,
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

    // Make the current committee.
    let committee = fixture.committee();

    let (_, certificates_store, payload_store) = create_db_stores();
    let (tx_header_waiter, _rx_header_waiter) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, _rx_certificate_waiter) = test_utils::test_channel!(1);

    let mut synchronizer = Synchronizer::new(
        name,
        &committee,
        certificates_store.clone(),
        payload_store,
        tx_header_waiter,
        tx_certificate_waiter,
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

    // Make the current committee.
    let committee = fixture.committee();

    let (_, certificates_store, payload_store) = create_db_stores();
    let (tx_header_waiter, _rx_header_waiter) = test_utils::test_channel!(1);
    let (tx_certificate_waiter, mut rx_certificate_waiter) = test_utils::test_channel!(1);

    let mut synchronizer = Synchronizer::new(
        name,
        &committee,
        certificates_store.clone(),
        payload_store,
        tx_header_waiter,
        tx_certificate_waiter,
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
