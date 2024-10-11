// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::LocalNarwhalClient;
use crate::{metrics::initialise_metrics, TrivialTransactionValidator};
use async_trait::async_trait;
use bytes::Bytes;
use fastcrypto::hash::Hash;
use futures::stream::FuturesOrdered;
use futures::StreamExt;
use primary::{CHANNEL_CAPACITY, NUM_SHUTDOWN_RECEIVERS};
use prometheus::Registry;
use store::rocks;
use store::rocks::MetricConf;
use store::rocks::ReadWriteOptions;
use test_utils::{
    batch, latest_protocol_version, temp_dir, test_network, transaction, CommitteeFixture,
};
use types::{
    BatchAPI, MockWorkerToPrimary, MockWorkerToWorker, PreSubscribedBroadcastSender,
    TransactionProto, TransactionsClient, WorkerBatchMessage, WorkerToWorkerClient,
};

// A test validator that rejects every transaction / batch
#[derive(Clone)]
struct NilTxValidator;
#[async_trait]
impl TransactionValidator for NilTxValidator {
    type Error = eyre::Report;

    fn validate(&self, _tx: &[u8]) -> Result<(), Self::Error> {
        eyre::bail!("Invalid transaction");
    }
    fn validate_batch(
        &self,
        _txs: &Batch,
        _protocol_config: &ProtocolConfig,
    ) -> Result<(), Self::Error> {
        eyre::bail!("Invalid batch");
    }
}

#[tokio::test]
async fn reject_invalid_clients_transactions() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let worker_id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(worker_id);
    let public_key = my_primary.public_key();
    let client = NetworkClient::new_from_keypair(&my_primary.network_keypair());

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // Create a new test store.
    let batch_store = rocks::DBMap::<BatchDigest, Batch>::open(
        temp_dir(),
        MetricConf::default(),
        None,
        Some("batches"),
        &ReadWriteOptions::default(),
    )
    .unwrap();

    let registry = Registry::new();
    let metrics = initialise_metrics(&registry);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance with a reject-all validator.
    Worker::spawn(
        my_primary.authority().clone(),
        myself.keypair(),
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        latest_protocol_version(),
        parameters,
        NilTxValidator,
        client,
        batch_store,
        metrics,
        &mut tx_shutdown,
    );

    // Wait till other services have been able to start up
    tokio::task::yield_now().await;
    // Send enough transactions to create a batch.
    let address = worker_cache
        .worker(&public_key, &worker_id)
        .unwrap()
        .transactions;
    let config = mysten_network::config::Config::new();
    let channel = config.connect_lazy(&address).unwrap();
    let mut client = TransactionsClient::new(channel);
    let tx = transaction();
    let txn = TransactionProto {
        transactions: vec![Bytes::from(tx.clone())],
    };

    // Check invalid transactions are rejected
    let res = client.submit_transaction(txn).await;
    assert!(res.is_err());

    let worker_pk = worker_cache.worker(&public_key, &worker_id).unwrap().name;

    let batch = batch(&latest_protocol_version());
    let batch_message = WorkerBatchMessage {
        batch: batch.clone(),
    };

    // setup network : impersonate a send from another worker
    let another_primary = fixture.authorities().nth(2).unwrap();
    let another_worker = another_primary.worker(worker_id);
    let network = test_network(
        another_worker.keypair(),
        &another_worker.info().worker_address,
    );
    // ensure that the networks are connected
    network
        .connect(myself.info().worker_address.to_anemo_address().unwrap())
        .await
        .unwrap();
    let peer = network.peer(PeerId(worker_pk.0.to_bytes())).unwrap();

    // Check invalid batches are rejected
    let res = WorkerToWorkerClient::new(peer)
        .report_batch(batch_message)
        .await;
    assert!(res.is_err());
}

/// TODO: test both RemoteNarwhalClient and LocalNarwhalClient in the same test case.
#[tokio::test]
async fn handle_remote_clients_transactions() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let worker_id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(worker_id);
    let authority_public_key = my_primary.public_key();
    let client = NetworkClient::new_from_keypair(&my_primary.network_keypair());

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // Create a new test store.
    let batch_store = rocks::DBMap::<BatchDigest, Batch>::open(
        temp_dir(),
        MetricConf::default(),
        None,
        Some("batches"),
        &ReadWriteOptions::default(),
    )
    .unwrap();

    let registry = Registry::new();
    let metrics = initialise_metrics(&registry);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance.
    Worker::spawn(
        my_primary.authority().clone(),
        myself.keypair(),
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        latest_protocol_version(),
        parameters,
        TrivialTransactionValidator,
        client.clone(),
        batch_store,
        metrics,
        &mut tx_shutdown,
    );

    // Spawn a network listener to receive our batch's digest.
    let mut peer_networks = Vec::new();

    // Create batches
    let batch = batch(&latest_protocol_version());
    let batch_digest = batch.digest();

    let (tx_await_batch, mut rx_await_batch) = test_utils::test_channel!(CHANNEL_CAPACITY);
    let mut mock_primary_server = MockWorkerToPrimary::new();
    mock_primary_server
        .expect_report_own_batch()
        .withf(move |request| {
            let message = request.body();

            message.digest == batch_digest && message.worker_id == worker_id
        })
        .times(1)
        .returning(move |_| {
            tx_await_batch.try_send(()).unwrap();
            Ok(anemo::Response::new(()))
        });
    client.set_worker_to_primary_local_handler(Arc::new(mock_primary_server));

    // Spawn enough workers' listeners to acknowledge our batches.
    for worker in fixture.authorities().skip(1).map(|a| a.worker(worker_id)) {
        let mut mock_server = MockWorkerToWorker::new();
        mock_server
            .expect_report_batch()
            .returning(|_| Ok(anemo::Response::new(())));
        let routes = anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(mock_server));
        peer_networks.push(worker.new_network(routes));
    }

    // Wait till other services have been able to start up
    tokio::task::yield_now().await;
    // Send enough transactions to create a batch.
    let address = worker_cache
        .worker(&authority_public_key, &worker_id)
        .unwrap()
        .transactions;
    let config = mysten_network::config::Config::new();
    let channel = config.connect_lazy(&address).unwrap();
    let client = TransactionsClient::new(channel);

    let join_handle = tokio::task::spawn(async move {
        let mut fut_list = FuturesOrdered::new();
        for tx in batch.transactions() {
            let txn = TransactionProto {
                transactions: vec![Bytes::from(tx.clone())],
            };

            // Calls to submit_transaction are now blocking, so we need to drive them
            // all at the same time, rather than sequentially.
            let mut inner_client = client.clone();
            fut_list.push_back(async move {
                inner_client.submit_transaction(txn).await.unwrap();
            });
        }

        // Drive all sending in parallel.
        while fut_list.next().await.is_some() {}
    });

    // Ensure the primary received the batch's digest (ie. it did not panic).
    rx_await_batch.recv().await.unwrap();

    // Ensure sending ended.
    assert!(join_handle.await.is_ok());
}

/// TODO: test both RemoteNarwhalClient and LocalNarwhalClient in the same test case.
#[tokio::test]
async fn handle_local_clients_transactions() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.worker_cache();

    let worker_id = 0;
    let my_primary = fixture.authorities().next().unwrap();
    let myself = my_primary.worker(worker_id);
    let authority_public_key = my_primary.public_key();
    let client = NetworkClient::new_from_keypair(&my_primary.network_keypair());

    let parameters = Parameters {
        batch_size: 200, // Two transactions.
        ..Parameters::default()
    };

    // Create a new test store.
    let batch_store = rocks::DBMap::<BatchDigest, Batch>::open(
        temp_dir(),
        MetricConf::default(),
        None,
        Some("batches"),
        &ReadWriteOptions::default(),
    )
    .unwrap();

    let registry = Registry::new();
    let metrics = initialise_metrics(&registry);

    let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

    // Spawn a `Worker` instance.
    Worker::spawn(
        my_primary.authority().clone(),
        myself.keypair(),
        worker_id,
        committee.clone(),
        worker_cache.clone(),
        latest_protocol_version(),
        parameters,
        TrivialTransactionValidator,
        client.clone(),
        batch_store,
        metrics,
        &mut tx_shutdown,
    );

    // Spawn a network listener to receive our batch's digest.
    let mut peer_networks = Vec::new();

    // Create batches
    let batch = batch(&latest_protocol_version());
    let batch_digest = batch.digest();

    let (tx_await_batch, mut rx_await_batch) = test_utils::test_channel!(CHANNEL_CAPACITY);
    let mut mock_primary_server = MockWorkerToPrimary::new();
    mock_primary_server
        .expect_report_own_batch()
        .withf(move |request| {
            let message = request.body();
            message.digest == batch_digest && message.worker_id == worker_id
        })
        .times(1)
        .returning(move |_| {
            tx_await_batch.try_send(()).unwrap();
            Ok(anemo::Response::new(()))
        });
    client.set_worker_to_primary_local_handler(Arc::new(mock_primary_server));

    // Spawn enough workers' listeners to acknowledge our batches.
    for worker in fixture.authorities().skip(1).map(|a| a.worker(worker_id)) {
        let mut mock_server = MockWorkerToWorker::new();
        mock_server
            .expect_report_batch()
            .returning(|_| Ok(anemo::Response::new(())));
        let routes = anemo::Router::new().add_rpc_service(WorkerToWorkerServer::new(mock_server));
        peer_networks.push(worker.new_network(routes));
    }

    // Wait till other services have been able to start up
    tokio::task::yield_now().await;
    // Send enough transactions to create a batch.
    let address = worker_cache
        .worker(&authority_public_key, &worker_id)
        .unwrap()
        .transactions;
    let client = LocalNarwhalClient::get_global(&address).unwrap().load();

    let join_handle = tokio::task::spawn(async move {
        let mut fut_list = FuturesOrdered::new();
        for txn in batch.transactions() {
            // Calls to submit_transaction are now blocking, so we need to drive them
            // all at the same time, rather than sequentially.
            let inner_client = client.clone();
            fut_list.push_back(async move {
                inner_client
                    .submit_transactions(vec![txn.clone()])
                    .await
                    .unwrap();
            });
        }

        // Drive all sending in parallel.
        while fut_list.next().await.is_some() {}
    });

    // Ensure the primary received the batch's digest (ie. it did not panic).
    rx_await_batch.recv().await.unwrap();

    // Ensure sending ended.
    assert!(join_handle.await.is_ok());
}
