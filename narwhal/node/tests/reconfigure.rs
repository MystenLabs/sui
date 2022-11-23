// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::mutable_key_type)]

use arc_swap::ArcSwap;
use bytes::Bytes;
use config::{Committee, Parameters, SharedWorkerCache, WorkerCache, WorkerId};
use crypto::{KeyPair, NetworkKeyPair, PublicKey};
use executor::{ExecutionIndices, ExecutionState};
use fastcrypto::traits::KeyPair as _;
use futures::future::join_all;
use narwhal_node as node;
use node::{restarter::NodeRestarter, Node};
use prometheus::Registry;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use storage::NodeStorage;
use test_utils::CommitteeFixture;
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{interval, sleep, Duration, MissedTickBehavior},
};
use types::ConsensusOutput;
use types::{ReconfigureNotification, TransactionProto, TransactionsClient};
use worker::TrivialTransactionValidator;

/// A simple/dumb execution engine.
struct SimpleExecutionState {
    keypair: KeyPair,
    network_keypair: NetworkKeyPair,
    worker_keypairs: Vec<NetworkKeyPair>,
    worker_cache: WorkerCache,
    committee: Arc<Mutex<Committee>>,
    tx_output: Sender<u64>,
    tx_reconfigure: Sender<(
        KeyPair,
        NetworkKeyPair,
        Committee,
        Vec<(WorkerId, NetworkKeyPair)>,
        WorkerCache,
    )>,
}

impl SimpleExecutionState {
    pub fn new(
        keypair: KeyPair,
        network_keypair: NetworkKeyPair,
        worker_keypairs: Vec<NetworkKeyPair>,
        worker_cache: WorkerCache,
        committee: Committee,
        tx_output: Sender<u64>,
        tx_reconfigure: Sender<(
            KeyPair,
            NetworkKeyPair,
            Committee,
            Vec<(WorkerId, NetworkKeyPair)>,
            WorkerCache,
        )>,
    ) -> Self {
        Self {
            keypair,
            network_keypair,
            worker_keypairs,
            worker_cache,
            committee: Arc::new(Mutex::new(committee)),
            tx_output,
            tx_reconfigure,
        }
    }
}

#[async_trait::async_trait]
impl ExecutionState for SimpleExecutionState {
    async fn handle_consensus_transaction(
        &self,
        _consensus_output: &Arc<ConsensusOutput>,
        execution_indices: ExecutionIndices,
        transaction: Vec<u8>,
    ) {
        let transaction: u64 = bincode::deserialize(&transaction).unwrap();
        // Change epoch every few certificates. Note that empty certificates are not provided to
        // this function (they are immediately skipped).
        let mut epoch = self.committee.lock().unwrap().epoch();
        if transaction >= epoch && execution_indices.next_certificate_index % 3 == 0 {
            epoch += 1;
            {
                let mut guard = self.committee.lock().unwrap();
                guard.epoch = epoch;
            };

            let worker_keypairs = self.worker_keypairs.iter().map(|kp| kp.copy());
            let worker_ids = 0..self.worker_keypairs.len() as u32;
            let worker_ids_and_keypairs = worker_ids.zip(worker_keypairs).collect();

            let new_committee = self.committee.lock().unwrap().clone();

            self.tx_reconfigure
                .send((
                    self.keypair.copy(),
                    self.network_keypair.copy(),
                    new_committee,
                    worker_ids_and_keypairs,
                    self.worker_cache.clone(),
                ))
                .await
                .unwrap();
        }

        let _ = self.tx_output.send(epoch).await;
    }

    async fn load_execution_indices(&self) -> ExecutionIndices {
        ExecutionIndices::default()
    }
}

async fn run_client(
    name: PublicKey,
    worker_cache: SharedWorkerCache,
    mut rx_reconfigure: Receiver<u64>,
) {
    let target = worker_cache
        .load()
        .worker(&name, /* id */ &0)
        .expect("Our key or worker id is not in the worker cache")
        .transactions;
    let config = mysten_network::config::Config::new();
    let channel = config.connect_lazy(&target).unwrap();
    let mut client = TransactionsClient::new(channel);

    // Make a transaction to submit for ever.
    let mut tx = TransactionProto {
        transaction: Bytes::from(0u64.to_be_bytes().to_vec()),
    };

    // Repeatedly send transactions.
    let mut interval = interval(Duration::from_millis(50));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    tokio::pin!(interval);

    loop {
        tokio::select! {
            // Wait a bit before repeating.
            _ = interval.tick() => {
                // Send a transactions.
                if client.submit_transaction(tx.clone()).await.is_err() {
                    // The workers are still down.
                    sleep(Duration::from_millis(100)).await;
                }
            },

            // Send transactions on the new epoch.
            Some(epoch) = rx_reconfigure.recv() => {
                tx = TransactionProto {
                    transaction: Bytes::from(epoch.to_le_bytes().to_vec()),
                };
            }
        }
    }
}

#[tokio::test]
async fn restart() {
    telemetry_subscribers::init_for_testing();
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();

    let parameters = Parameters {
        batch_size: 200,
        max_header_num_of_batches: 1,
        ..Parameters::default()
    };

    // Spawn the nodes.
    let mut rx_nodes = Vec::new();
    for a in fixture.authorities() {
        let (tx_output, rx_output) = channel(10);
        let (tx_node_reconfigure, rx_node_reconfigure) = channel(10);

        let execution_state = Arc::new(SimpleExecutionState::new(
            a.keypair().copy(),
            a.network_keypair().copy(),
            a.worker_keypairs(),
            fixture.worker_cache(),
            committee.clone(),
            tx_output,
            tx_node_reconfigure,
        ));

        let worker_keypairs = a.worker_keypairs();
        let worker_ids = 0..worker_keypairs.len() as u32;
        let worker_ids_and_keypairs = worker_ids.zip(worker_keypairs.into_iter()).collect();

        let committee = committee.clone();
        let worker_cache = worker_cache.clone();
        let parameters = parameters.clone();
        let keypair = a.keypair().copy();
        let network_keypair = a.network_keypair().copy();
        tokio::spawn(async move {
            NodeRestarter::watch(
                keypair,
                network_keypair,
                worker_ids_and_keypairs,
                &committee,
                worker_cache,
                /* base_store_path */ test_utils::temp_dir(),
                execution_state,
                parameters,
                TrivialTransactionValidator::default(),
                rx_node_reconfigure,
                &Registry::new(),
            )
            .await;
        });

        rx_nodes.push(rx_output);
    }

    // Give a chance to the nodes to start.
    tokio::task::yield_now().await;

    // Spawn some clients.
    let mut tx_clients = Vec::new();
    for a in fixture.authorities() {
        let (tx_client_reconfigure, rx_client_reconfigure) = channel(10);
        tx_clients.push(tx_client_reconfigure);

        let name = a.public_key();
        let worker_cache = worker_cache.clone();
        tokio::spawn(
            async move { run_client(name, worker_cache.clone(), rx_client_reconfigure).await },
        );
    }

    // Listen to the outputs.
    let mut handles = Vec::new();
    for (tx, mut rx) in tx_clients.into_iter().zip(rx_nodes.into_iter()) {
        handles.push(tokio::spawn(async move {
            let mut current_epoch = 0u64;
            while let Some(epoch) = rx.recv().await {
                if epoch == 5 {
                    return;
                }
                if epoch > current_epoch {
                    current_epoch = epoch;
                    tx.send(current_epoch).await.unwrap();
                }
            }
        }));
    }
    join_all(handles).await;
}

#[tokio::test]
async fn epoch_change() {
    let fixture = CommitteeFixture::builder().randomize_ports(true).build();
    let committee = fixture.committee();
    let worker_cache = fixture.shared_worker_cache();
    let parameters = fixture
        .authorities()
        .map(|a| {
            (
                a.public_key(),
                Parameters {
                    batch_size: 200,
                    header_num_of_batches_threshold: 1, // One batch digest
                    ..Parameters::default()
                },
            )
        })
        .collect::<HashMap<_, _>>();

    // Spawn the nodes.
    let mut rx_nodes = Vec::new();
    for a in fixture.authorities() {
        let (tx_output, rx_output) = channel(10);
        let (tx_node_reconfigure, mut rx_node_reconfigure) = channel(10);

        let name = a.public_key();
        let store = NodeStorage::reopen(test_utils::temp_dir());

        let execution_state = Arc::new(SimpleExecutionState::new(
            a.keypair().copy(),
            a.network_keypair().copy(),
            a.worker_keypairs(),
            fixture.worker_cache(),
            committee.clone(),
            tx_output,
            tx_node_reconfigure,
        ));

        // Start a task that will broadcast the committee change signal.
        let parameters_clone = parameters.get(&name).unwrap().clone();
        tokio::spawn(async move {
            let client = reqwest::Client::new();

            while let Some((_, _, committee, _, _)) = rx_node_reconfigure.recv().await {
                let message = ReconfigureNotification::NewEpoch(committee.clone());
                client
                    .post(format!(
                        "http://127.0.0.1:{}/reconfigure",
                        parameters_clone
                            .network_admin_server
                            .primary_network_admin_server_port
                    ))
                    .json(&message)
                    .send()
                    .await
                    .unwrap();
            }
        });

        let p = parameters.get(&name).unwrap().clone();
        let _primary_handles = Node::spawn_primary(
            a.keypair().copy(),
            a.network_keypair().copy(),
            Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
            worker_cache.clone(),
            &store,
            p.clone(),
            /* consensus */ true,
            execution_state,
            &Registry::new(),
        )
        .await
        .unwrap();

        let _worker_handles = Node::spawn_workers(
            name,
            /* worker ids_and_keypairs */ vec![(0, a.worker(0).keypair().copy())],
            Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
            worker_cache.clone(),
            &store,
            p,
            TrivialTransactionValidator::default(),
            &Registry::new(),
        );

        rx_nodes.push(rx_output);
    }

    // Give a chance to the nodes to start.
    tokio::task::yield_now().await;

    // Spawn some clients.
    let mut tx_clients = Vec::new();
    for a in fixture.authorities() {
        let (tx_client_reconfigure, rx_client_reconfigure) = channel(10);
        tx_clients.push(tx_client_reconfigure);

        let name = a.public_key();
        let worker_cache = worker_cache.clone();
        tokio::spawn(
            async move { run_client(name, worker_cache.clone(), rx_client_reconfigure).await },
        );
    }

    // Listen to the outputs.
    let mut handles = Vec::new();
    for (tx, mut rx) in tx_clients.into_iter().zip(rx_nodes.into_iter()) {
        handles.push(tokio::spawn(async move {
            let mut current_epoch = 0u64;
            while let Some(epoch) = rx.recv().await {
                if epoch == 5 {
                    return;
                }
                if epoch > current_epoch {
                    current_epoch = epoch;
                    tx.send(current_epoch).await.unwrap();
                }
            }
        }));
    }
    join_all(handles).await;
}
