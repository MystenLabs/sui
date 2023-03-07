// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Dag, ValidatorDagError};
use crate::NUM_SHUTDOWN_RECEIVERS;
use dag::node_dag::NodeDagError;
use fastcrypto::hash::Hash;
use indexmap::IndexMap;
use std::{collections::BTreeSet, sync::Arc};
use test_utils::{make_optimal_certificates, CommitteeFixture};
use types::{Certificate, CertificateAPI, HeaderAPI, PreSubscribedBroadcastSender};

#[tokio::test]
async fn inner_dag_insert_one() {
    // Make certificates for rounds 1 to 4.
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _next_parents) =
        make_optimal_certificates(&committee, 1..=4, &genesis, &ids);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // set up a Dag
    let (tx_cert, rx_cert) = test_utils::test_channel!(1);
    Dag::new(&committee, rx_cert, tx_shutdown.subscribe());

    // Feed the certificates to the Dag
    while let Some(certificate) = certificates.pop_front() {
        tx_cert.send(certificate).await.unwrap();
    }
}
#[tokio::test]
async fn test_dag_read_notify() {
    // Make certificates for rounds 1 to 4.
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _next_parents) =
        make_optimal_certificates(&committee, 1..=4, &genesis, &ids);
    let certs = certificates.clone().into_iter().map(|c| (c.digest(), c));
    // set up a Dag
    let (_tx_cert, rx_cert) = test_utils::test_channel!(1);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);
    let arc = Arc::new(Dag::new(&committee, rx_cert, tx_shutdown.subscribe()));
    let cloned = arc.clone();
    let handle = tokio::spawn(async move {
        let _ = &arc;
        for (digest, cert) in certs {
            match arc.1.notify_read(digest).await {
                Ok(v) => assert_eq!(v, cert),
                _ => panic!("Failed to read from store"),
            }
        }
    });

    // Feed the certificates to the Dag
    while let Some(certificate) = certificates.pop_front() {
        cloned.1.insert(certificate).await.unwrap();
    }
    assert!(handle.await.is_ok());
}

#[tokio::test]
async fn test_dag_new_has_genesis_and_its_not_live() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    // set up a Dag
    let (_tx_cert, rx_cert) = test_utils::test_channel!(1);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let (_, dag) = Dag::new(&committee, rx_cert, tx_shutdown.subscribe());

    for certificate in genesis.clone() {
        assert!(dag.contains(certificate).await);
    }

    // But the genesis does not come out in read_causal, as is is compressed the moment we add more nodes
    let (certificates, _next_parents) =
        make_optimal_certificates(&committee, 1..=1, &genesis, &ids);
    let mut certs_to_insert = certificates.clone();

    // Feed the additional certificates to the Dag
    while let Some(certificate) = certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // genesis is still here
    for certificate in genesis.clone() {
        assert!(dag.contains(certificate).await);
    }

    // we trigger read_causal on the newly inserted cert
    for cert in certificates.clone() {
        let res = dag.read_causal(cert.digest()).await.unwrap();
        // the read_causals do not report genesis: we only walk one node, the start of the walk
        assert_eq!(res, vec![cert.digest()]);
    }

    // genesis is no longer here
    for certificate in genesis {
        assert!(!dag.contains(certificate).await);
    }
}

// `test_dag_new_has_genesis_and_its_not_live` relies on the fact that genesis produces empty blocks: we re-run it with non-genesis empty blocks to
// check the invariants are the same
#[tokio::test]
async fn test_dag_compresses_empty_blocks() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    // set up a Dag
    let (_tx_cert, rx_cert) = test_utils::test_channel!(1);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let (_, dag) = Dag::new(&committee, rx_cert, tx_shutdown.subscribe());

    // insert one round of empty certificates
    let (mut certificates, next_parents) =
        make_optimal_certificates(&committee, 1..=1, &genesis.clone(), &ids);
    // make those empty
    for cert in certificates.iter_mut() {
        cert.header_mut().update_payload(IndexMap::new());
    }

    // Feed the certificates to the Dag
    let mut certs_to_insert = certificates.clone();
    while let Some(certificate) = certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // certificates are still here
    for certificate in certificates.clone() {
        assert!(dag.contains(certificate.digest()).await);
    }

    // Add one round of non-empty certificates
    let (additional_certificates, _next_parents) =
        make_optimal_certificates(&committee, 2..=2, &next_parents, &ids);
    // Feed the additional certificates to the Dag
    let mut additional_certs_to_insert = additional_certificates.clone();
    while let Some(certificate) = additional_certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // we trigger read_causal on all the newly inserted certs
    for cert in additional_certificates.clone() {
        let res = dag.read_causal(cert.digest()).await.unwrap();
        // the read_causals do not report genesis or the empty round we inserted: we only walk one node, the start of the walk
        assert_eq!(res, vec![cert.digest()]);
    }

    // genesis is gone
    for digest in genesis {
        assert!(!dag.contains(digest).await);
    }

    // certificates are gone
    for certificate in certificates {
        assert!(
            !dag.contains(certificate.digest()).await,
            "{} should no longer be here",
            certificate.digest()
        );
    }
}

#[tokio::test]
async fn test_dag_rounds_after_compression() {
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    // set up a Dag
    let (_tx_cert, rx_cert) = test_utils::test_channel!(1);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let (_, dag) = Dag::new(&committee, rx_cert, tx_shutdown.subscribe());

    // insert one round of empty certificates
    let (mut certificates, next_parents) =
        make_optimal_certificates(&committee, 1..=1, &genesis.clone(), &ids);
    // make those empty
    for cert in certificates.iter_mut() {
        cert.header_mut().update_payload(IndexMap::new());
    }

    // Feed the certificates to the Dag
    let mut certs_to_insert = certificates.clone();
    while let Some(certificate) = certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // Add one round of non-empty certificates
    let (additional_certificates, _next_parents) =
        make_optimal_certificates(&committee, 2..=2, &next_parents, &ids);
    // Feed the additional certificates to the Dag
    let mut additional_certs_to_insert = additional_certificates.clone();
    while let Some(certificate) = additional_certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // Do not trigger read_causal on all the newly inserted certs
    // Test rounds: they reflect that the round of compressible certificates is gone
    for pub_key in ids {
        assert_eq!(dag.rounds(pub_key).await.unwrap(), 2..=2)
    }
}

#[tokio::test]
async fn dag_mutation_failures() {
    // Make certificates for rounds 1 to 4.
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (certificates, _next_parents) =
        make_optimal_certificates(&committee, 1..=4, &genesis, &ids);

    // set up a Dag
    let (_tx_cert, rx_cert) = test_utils::test_channel!(1);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    let (_handle, dag) = Dag::new(&committee, rx_cert, tx_shutdown.subscribe());
    let mut certs_to_insert = certificates.clone();
    let mut certs_to_insert_in_reverse = certs_to_insert.clone();
    let mut certs_to_remove_before_insert = certs_to_insert.clone();

    // Removing unknown certificates should fail
    while let Some(certificate) = certs_to_remove_before_insert.pop_back() {
        assert!(matches!(
            dag.remove(vec![certificate.digest()]).await,
            Err(ValidatorDagError::DagInvariantViolation(
                NodeDagError::UnknownDigests(_)
            ))
        ))
    }

    // Feed the certificates to the Dag in reverse order, triggering missing parent errors for all but the last round
    while let Some(certificate) = certs_to_insert_in_reverse.pop_back() {
        if certificate.round() != 1 {
            assert!(matches!(
                dag.insert(certificate).await,
                Err(ValidatorDagError::DagInvariantViolation(
                    NodeDagError::UnknownDigests(_)
                ))
            ))
        }
    }

    // Check no authority has live vertexes beyond 1
    for authority in ids.clone() {
        assert_eq!(dag.rounds(authority).await.unwrap(), 0..=0)
    }

    // Feed the certificates to the Dag in order
    while let Some(certificate) = certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // Check all authorities have live vertexes 1..=4 (genesis is compressible)
    for authority in ids.clone() {
        assert_eq!(dag.rounds(authority).await.unwrap(), 1..=4)
    }

    // We have only inserted from round 0 to 4 => round 5 queries should fail
    for authority in ids {
        assert!(matches!(
            dag.node_read_causal(authority, 5).await,
            Err(ValidatorDagError::NoCertificateForCoordinates(_, 5))
        ))
    }
}

#[tokio::test]
async fn dag_insert_one_and_rounds_node_read() {
    // Make certificates for rounds 1 to 4.
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (certificates, _next_parents) =
        make_optimal_certificates(&committee, 1..=4, &genesis, &ids);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // set up a Dag
    let (_tx_cert, rx_cert) = test_utils::test_channel!(1);
    let (_handle, dag) = Dag::new(&committee, rx_cert, tx_shutdown.subscribe());
    let mut certs_to_insert = certificates.clone();

    // Feed the certificates to the Dag
    while let Some(certificate) = certs_to_insert.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // we fed 4 complete rounds, and genesis is compressible => rounds(pk) = 1..=4
    for authority in ids.clone() {
        assert_eq!(1..=4, dag.rounds(authority).await.unwrap());
    }

    // on optimal certificates (we ack all of the prior round), we BFT 1 + 3 * 4 vertices:
    // as genesis is compressible, that initial round is omitted
    for certificate in certificates {
        if certificate.round() == 4 {
            assert_eq!(
                13,
                dag.read_causal(certificate.digest()).await.unwrap().len()
            );
        }
    }

    // on optimal certificates (we ack all of the prior round), we BFT 1 + 3 * 4 vertices
    for authority in ids {
        assert_eq!(13, dag.node_read_causal(authority, 4).await.unwrap().len());
    }
}

#[tokio::test]
async fn dag_insert_and_remove_reads() {
    // Make certificates for rounds 1 to 4.
    let fixture = CommitteeFixture::builder().build();
    let committee = fixture.committee();
    let ids: Vec<_> = fixture.authorities().map(|a| a.id()).collect();
    let mut genesis_certs = Certificate::genesis(&committee);
    let genesis = genesis_certs
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, _next_parents) =
        make_optimal_certificates(&committee, 1..=4, &genesis, &ids);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // set up a Dag
    let (_tx_cert, rx_cert) = test_utils::test_channel!(1);
    let (_handle, dag) = Dag::new(&committee, rx_cert, tx_shutdown.subscribe());

    // Feed the certificates to the Dag
    while let Some(certificate) = certificates.pop_front() {
        dag.insert(certificate).await.unwrap();
    }

    // we remove round 0
    while let Some(genesis_cert) = genesis_certs.pop() {
        dag.remove(vec![genesis_cert.digest()]).await.unwrap();
    }

    // on optimal certificates (we ack all of the prior round), we BFT 1 + 3 * 4 vertices
    // (round 0 disappeared)
    for authority in ids {
        assert_eq!(13, dag.node_read_causal(authority, 4).await.unwrap().len());
    }

    // Ensure that the dag will reply true when we check whether we have seen
    // all the removed certificates.
    for digest in genesis {
        assert!(dag.has_ever_contained(digest).await);
    }
}
