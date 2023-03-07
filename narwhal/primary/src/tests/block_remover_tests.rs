// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{block_remover::BlockRemover, common::create_db_stores, NUM_SHUTDOWN_RECEIVERS};
use anemo::PeerId;
use config::{Committee, WorkerId};
use consensus::dag::Dag;
use crypto::traits::KeyPair;
use fastcrypto::hash::Hash;
use futures::future::join_all;
use std::{borrow::Borrow, collections::HashMap, sync::Arc, time::Duration};
use test_utils::CommitteeFixture;
use tokio::time::timeout;
use types::{
    BatchDigest, Certificate, Header, MockPrimaryToWorker, PreSubscribedBroadcastSender,
    PrimaryToWorkerServer, WorkerDeleteBatchesMessage,
};

#[tokio::test]
async fn test_successful_blocks_delete() {
    // GIVEN
    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (_tx_consensus, rx_consensus) = test_utils::test_channel!(1);
    let (tx_removed_certificates, mut rx_removed_certificates) = test_utils::test_channel!(10);

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // AND a Dag with genesis populated
    let dag = Arc::new(Dag::new(&committee, rx_consensus, tx_shutdown.subscribe()).1);
    populate_genesis(&dag, &committee).await;

    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let block_remover = BlockRemover::new(
        id,
        committee.clone(),
        worker_cache.clone(),
        certificate_store.clone(),
        header_store.clone(),
        payload_store.clone(),
        Some(dag.clone()),
        network.clone(),
        tx_removed_certificates,
    );

    let mut digests = Vec::new();
    let mut header_ids = Vec::new();

    let mut worker_batches: HashMap<WorkerId, Vec<BatchDigest>> = HashMap::new();

    let worker_id_0 = 0;
    let worker_id_1 = 1;

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _headers in 0..5 {
        let batch_1 = test_utils::fixture_batch_with_transactions(10);
        let batch_2 = test_utils::fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch_1.clone(), worker_id_0, 0)
                .with_payload_batch(batch_2.clone(), worker_id_1, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.digest();

        // write the certificate
        certificate_store.write(certificate.clone()).unwrap();
        dag.insert(certificate).await.unwrap();

        // write the header
        header_store.write(&header).unwrap();

        header_ids.push(header.clone().digest());

        // write the batches to payload store
        payload_store
            .write_all(vec![
                (batch_1.clone().digest(), worker_id_0),
                (batch_2.clone().digest(), worker_id_1),
            ])
            .expect("couldn't store batches");

        digests.push(digest);

        worker_batches
            .entry(worker_id_0)
            .or_insert_with(Vec::new)
            .push(batch_1.digest());

        worker_batches
            .entry(worker_id_1)
            .or_insert_with(Vec::new)
            .push(batch_2.digest());
    }

    // AND bootstrap the workers
    let mut worker_networks = Vec::new();
    for (worker_id, batch_digests) in worker_batches.clone() {
        let worker = primary.worker(worker_id);
        let address = &worker.info().worker_address;

        let mut mock_server = MockPrimaryToWorker::new();
        mock_server
            .expect_delete_batches()
            .withf(move |request| {
                request.body()
                    == &WorkerDeleteBatchesMessage {
                        digests: batch_digests.clone(),
                    }
            })
            .returning(|_| Ok(anemo::Response::new(())));
        let routes = anemo::Router::new().add_rpc_service(PrimaryToWorkerServer::new(mock_server));
        worker_networks.push(worker.new_network(routes));

        let address = address.to_anemo_address().unwrap();
        let peer_id = PeerId(worker.keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    block_remover.remove_blocks(digests.clone()).await.unwrap();

    // ensure that certificates have been deleted from store
    for digest in digests.clone() {
        assert!(
            certificate_store.read(digest).unwrap().is_none(),
            "Certificate shouldn't exist"
        );
    }

    // ensure that headers have been deleted from store
    for header_id in header_ids {
        assert!(
            header_store.read(&header_id).unwrap().is_none(),
            "Header shouldn't exist"
        );
    }

    // ensure that batches have been deleted from the payload store
    for (worker_id, batch_digests) in worker_batches {
        for digest in batch_digests {
            assert!(
                !payload_store.contains(digest, worker_id).unwrap(),
                "Payload shouldn't exist"
            );
        }
    }

    // ensure deleted certificates have been populated to output channel
    let mut total_deleted = 0;

    while let Ok(Some((_round, certs))) =
        timeout(Duration::from_secs(1), rx_removed_certificates.recv()).await
    {
        for ci in certs {
            assert!(
                digests.contains(&ci.digest()),
                "Deleted certificate not found"
            );
            total_deleted += 1;
        }
    }

    assert_eq!(total_deleted, digests.len());
}

#[tokio::test]
async fn test_failed_blocks_delete() {
    // GIVEN
    let (header_store, certificate_store, payload_store) = create_db_stores();
    let (_tx_consensus, rx_consensus) = test_utils::test_channel!(1);
    let (tx_removed_certificates, mut rx_removed_certificates) = test_utils::test_channel!(10);
    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // AND the necessary keys
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();
    let author = fixture.authorities().next().unwrap();
    let primary = fixture.authorities().nth(1).unwrap();
    let id = primary.id();
    // AND a Dag with genesis populated
    let dag = Arc::new(Dag::new(&committee, rx_consensus, tx_shutdown.subscribe()).1);
    populate_genesis(&dag, &committee).await;

    let network = test_utils::test_network(primary.network_keypair(), primary.address());
    let block_remover = BlockRemover::new(
        id,
        committee.clone(),
        worker_cache.clone(),
        certificate_store.clone(),
        header_store.clone(),
        payload_store.clone(),
        Some(dag.clone()),
        network.clone(),
        tx_removed_certificates,
    );

    let mut digests = Vec::new();
    let mut header_ids = Vec::new();

    let mut worker_batches: HashMap<WorkerId, Vec<BatchDigest>> = HashMap::new();

    let worker_id_0 = 0;
    let worker_id_1 = 1;

    // AND generate headers with distributed batches between 2 workers (0 and 1)
    for _headers in 0..5 {
        let batch_1 = test_utils::fixture_batch_with_transactions(10);
        let batch_2 = test_utils::fixture_batch_with_transactions(10);

        let header = Header::V1(
            author
                .header_builder(&committee)
                .with_payload_batch(batch_1.clone(), worker_id_0, 0)
                .with_payload_batch(batch_2.clone(), worker_id_1, 0)
                .build()
                .unwrap(),
        );

        let certificate = fixture.certificate(&header);
        let digest = certificate.digest();

        // write the certificate
        certificate_store.write(certificate.clone()).unwrap();
        dag.insert(certificate).await.unwrap();

        // write the header
        header_store.write(&header).unwrap();

        header_ids.push(header.clone().digest());

        // write the batches to payload store
        payload_store
            .write_all(vec![
                (batch_1.clone().digest(), worker_id_0),
                (batch_2.clone().digest(), worker_id_1),
            ])
            .expect("couldn't store batches");

        digests.push(digest);

        worker_batches
            .entry(worker_id_0)
            .or_insert_with(Vec::new)
            .push(batch_1.digest());

        worker_batches
            .entry(worker_id_1)
            .or_insert_with(Vec::new)
            .push(batch_2.digest());
    }

    // AND bootstrap the workers
    let mut worker_networks = Vec::new();
    for (worker_id, batch_digests) in worker_batches.clone() {
        let worker = primary.worker(worker_id);
        let address = &worker.info().worker_address;

        let mut mock_server = MockPrimaryToWorker::new();
        mock_server
            .expect_delete_batches()
            .withf(move |request| {
                request.body()
                    == &WorkerDeleteBatchesMessage {
                        digests: batch_digests.clone(),
                    }
            })
            .returning(move |_| {
                if worker_id == 0 {
                    Err(anemo::rpc::Status::internal("failed"))
                } else {
                    Ok(anemo::Response::new(()))
                }
            });
        let routes = anemo::Router::new().add_rpc_service(PrimaryToWorkerServer::new(mock_server));
        worker_networks.push(worker.new_network(routes));

        let address = address.to_anemo_address().unwrap();
        let peer_id = PeerId(worker.keypair().public().0.to_bytes());
        network
            .connect_with_peer_id(address, peer_id)
            .await
            .unwrap();
    }

    assert!(block_remover.remove_blocks(digests.clone()).await.is_err());

    // Ensure that nothing else is deleted after failed worker batch delete.
    for digest in digests.clone() {
        assert!(certificate_store.read(digest).unwrap().is_some());
    }
    for header_id in header_ids {
        assert!(header_store.read(&header_id).unwrap().is_some());
    }
    for (worker_id, batch_digests) in worker_batches {
        for digest in batch_digests {
            assert!(payload_store.contains(digest, worker_id).unwrap());
        }
    }
    let mut total_deleted = 0;
    while let Ok(Some(_)) = timeout(Duration::from_secs(1), rx_removed_certificates.recv()).await {
        total_deleted += 1;
    }
    assert_eq!(total_deleted, 0);
}

async fn populate_genesis<K: Borrow<Dag>>(dag: &K, committee: &Committee) {
    assert!(join_all(
        Certificate::genesis(committee)
            .iter()
            .map(|cert| dag.borrow().insert(cert.clone())),
    )
    .await
    .iter()
    .all(|r| r.is_ok()));
}
