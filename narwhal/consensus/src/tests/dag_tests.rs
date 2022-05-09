// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crypto::{traits::KeyPair, Hash};
use tokio::sync::mpsc::channel;
use types::Certificate;

use crate::tusk::consensus_tests::{make_optimal_certificates, mock_committee};

use super::{Dag, InnerDag};

#[tokio::test]
async fn inner_dag_insert_one() {
    // Make certificates for rounds 1 to 4.
    let keys: Vec<_> = test_utils::keys()
        .into_iter()
        .map(|kp| kp.public().clone())
        .collect();
    let mut genesis_certs = Certificate::genesis(&mock_committee(&keys.clone()[..]));
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _next_parents) = make_optimal_certificates(1, 4, &genesis, &keys);

    // set up a Dag
    let (tx_cert, rx_cert) = channel(1);
    InnerDag::spawn(rx_cert);

    // Feed the certificates to the Dag
    while let Some(certificate) = genesis_certs.pop() {
        tx_cert.send(certificate).await.unwrap();
    }
    while let Some(certificate) = certificates.pop_front() {
        tx_cert.send(certificate).await.unwrap();
    }
}

#[tokio::test]
async fn dag_insert_one_and_rounds_node_read() {
    // Make certificates for rounds 1 to 4.
    let keys: Vec<_> = test_utils::keys()
        .into_iter()
        .map(|kp| kp.public().clone())
        .collect();
    let mut genesis_certs = Certificate::genesis(&mock_committee(&keys.clone()[..]));
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (certificates, _next_parents) = make_optimal_certificates(1, 4, &genesis, &keys);

    // set up a Dag
    let (_tx_cert, rx_cert) = channel(1);
    let (_handle, mut dag) = Dag::new(rx_cert);
    let mut certs_to_insert = certificates.clone();

    // Feed the certificates to the Dag
    while let Some(certificate) = genesis_certs.pop() {
        dag.insert(certificate).await.unwrap();
    }
    while let Some(certificate) = certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // we fed 4 complete rounds => rounds(pk) = 0..=4
    for authority in keys.clone() {
        assert_eq!(0..=4, dag.rounds(authority.clone()).await.unwrap());
    }

    // on optimal certificates (we ack all of the prior round), we BFT 1 + 4 * 4 vertices
    for certificate in certificates {
        if certificate.round() == 4 {
            assert_eq!(
                17,
                dag.read_causal(certificate.digest()).await.unwrap().len()
            );
        }
    }

    // on optimal certificates (we ack all of the prior round), we BFT 1 + 4 * 4 vertices
    for authority in keys {
        assert_eq!(17, dag.node_read_causal(authority, 4).await.unwrap().len());
    }
}

#[tokio::test]
async fn dag_insert_and_remove_reads() {
    // Make certificates for rounds 1 to 4.
    let keys: Vec<_> = test_utils::keys()
        .into_iter()
        .map(|kp| kp.public().clone())
        .collect();
    let mut genesis_certs = Certificate::genesis(&mock_committee(&keys.clone()[..]));
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _next_parents) = make_optimal_certificates(1, 4, &genesis, &keys);

    // set up a Dag
    let (_tx_cert, rx_cert) = channel(1);
    let (_handle, mut dag) = Dag::new(rx_cert);
    let mut genesis_certs_to_insert = genesis_certs.clone();

    // Feed the certificates to the Dag
    while let Some(certificate) = genesis_certs_to_insert.pop() {
        dag.insert(certificate).await.unwrap();
    }
    while let Some(certificate) = certificates.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // we remove round 0
    while let Some(genesis_cert) = genesis_certs.pop() {
        dag.remove(genesis_cert.digest()).await.unwrap();
    }

    // on optimal certificates (we ack all of the prior round), we BFT 1 + 3 * 4 vertices
    // (round 0 disappeared)
    for authority in keys {
        assert_eq!(13, dag.node_read_causal(authority, 4).await.unwrap().len());
    }
}
