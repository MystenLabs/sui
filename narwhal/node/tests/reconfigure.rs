// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use arc_swap::ArcSwap;
use bytes::Bytes;
use config::{Committee, Parameters};
use consensus::ConsensusOutput;
use crypto::{
    ed25519::{Ed25519KeyPair, Ed25519PublicKey},
    traits::{KeyPair, VerifyingKey},
};
use executor::{ExecutionIndices, ExecutionState, ExecutionStateError};
use futures::future::join_all;
use network::{PrimaryToWorkerNetwork, WorkerToPrimaryNetwork};
use node::{restarter::NodeRestarter, Node, NodeStorage};
use primary::PrimaryWorkerMessage;
use prometheus::Registry;
use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};
use test_utils::{committee, keys};
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::{interval, sleep, Duration, MissedTickBehavior},
};
use types::{ReconfigureNotification, TransactionProto, TransactionsClient, WorkerPrimaryMessage};

/// A simple/dumb execution engine.
struct SimpleExecutionState {
    index: usize,
    committee: Arc<Mutex<Committee<Ed25519PublicKey>>>,
    tx_reconfigure: Sender<(Ed25519KeyPair, Committee<Ed25519PublicKey>)>,
}

impl SimpleExecutionState {
    pub fn new(
        index: usize,
        committee: Committee<Ed25519PublicKey>,
        tx_reconfigure: Sender<(Ed25519KeyPair, Committee<Ed25519PublicKey>)>,
    ) -> Self {
        Self {
            index,
            committee: Arc::new(Mutex::new(committee)),
            tx_reconfigure,
        }
    }
}

#[async_trait::async_trait]
impl ExecutionState for SimpleExecutionState {
    type Transaction = u64;
    type Error = SimpleExecutionError;
    type Outcome = u64;

    async fn handle_consensus_transaction<PublicKey: VerifyingKey>(
        &self,
        _consensus_output: &ConsensusOutput<PublicKey>,
        execution_indices: ExecutionIndices,
        transaction: Self::Transaction,
    ) -> Result<(Self::Outcome, Option<Committee<PublicKey>>), Self::Error> {
        // Change epoch every few certificates. Note that empty certificates are not provided to
        // this function (they are immediately skipped).
        let mut epoch = self.committee.lock().unwrap().epoch();
        if transaction >= epoch && execution_indices.next_certificate_index % 3 == 0 {
            epoch += 1;
            {
                let mut guard = self.committee.lock().unwrap();
                guard.epoch = epoch;
            };

            let keypair = keys(None)
                .into_iter()
                .enumerate()
                .filter(|(i, _)| i == &self.index)
                .map(|(_, x)| x)
                .collect::<Vec<_>>()
                .pop()
                .unwrap();
            let new_committee = self.committee.lock().unwrap().clone();
            self.tx_reconfigure
                .send((keypair, new_committee))
                .await
                .unwrap();
        }

        Ok((epoch, None))
    }

    fn ask_consensus_write_lock(&self) -> bool {
        true
    }

    fn release_consensus_write_lock(&self) {}

    async fn load_execution_indices(&self) -> Result<ExecutionIndices, Self::Error> {
        Ok(ExecutionIndices::default())
    }
}

/// A simple/dumb execution error.
#[derive(Debug, thiserror::Error)]
pub enum SimpleExecutionError {
    #[error("Something went wrong in the authority")]
    ServerError,

    #[error("The client made something bad")]
    ClientError,
}

#[async_trait::async_trait]
impl ExecutionStateError for SimpleExecutionError {
    fn node_error(&self) -> bool {
        match self {
            Self::ServerError => true,
            Self::ClientError => false,
        }
    }

    fn to_string(&self) -> String {
        ToString::to_string(&self)
    }
}

async fn run_client<PublicKey: VerifyingKey>(
    name: PublicKey,
    committee: Committee<PublicKey>,
    mut rx_reconfigure: Receiver<u64>,
) {
    let target = committee
        .worker(&name, /* id */ &0)
        .expect("Our key or worker id is not in the committee")
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
    let committee = committee(None);
    let parameters = Parameters::default();

    // Spawn the nodes.
    let mut states = Vec::new();
    let mut rx_nodes = Vec::new();
    for (i, keypair) in keys(None).into_iter().enumerate() {
        let (tx_output, rx_output) = channel(10);
        let (tx_node_reconfigure, rx_node_reconfigure) = channel(10);

        let execution_state = Arc::new(SimpleExecutionState::new(
            i,
            committee.clone(),
            tx_node_reconfigure,
        ));
        states.push(execution_state.clone());

        let committee = committee.clone();
        let execution_state = execution_state.clone();
        let parameters = parameters.clone();
        tokio::spawn(async move {
            NodeRestarter::watch(
                keypair,
                &committee,
                /* base_store_path */ test_utils::temp_dir(),
                execution_state,
                parameters,
                rx_node_reconfigure,
                tx_output,
            )
            .await;
        });

        rx_nodes.push(rx_output);
    }

    // Give a chance to the nodes to start.
    tokio::task::yield_now().await;

    // Spawn some clients.
    let mut tx_clients = Vec::new();
    for keypair in keys(None) {
        let (tx_client_reconfigure, rx_client_reconfigure) = channel(10);
        tx_clients.push(tx_client_reconfigure);

        let name = keypair.public().clone();
        let committee = committee.clone();
        tokio::spawn(
            async move { run_client(name, committee.clone(), rx_client_reconfigure).await },
        );
    }

    // Listen to the outputs.
    let mut handles = Vec::new();
    for (tx, mut rx) in tx_clients.into_iter().zip(rx_nodes.into_iter()) {
        handles.push(tokio::spawn(async move {
            let mut current_epoch = 0u64;
            while let Some(output) = rx.recv().await {
                let (outcome, _tx) = output;
                match outcome {
                    Ok(epoch) => {
                        if epoch == 5 {
                            return;
                        }
                        if epoch > current_epoch {
                            current_epoch = epoch;
                            tx.send(current_epoch).await.unwrap();
                        }
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        }));
    }
    join_all(handles).await;
}

#[tokio::test]
async fn epoch_change() {
    let committee = committee(None);
    let parameters = Parameters::default();

    // Spawn the nodes.
    let mut states = Vec::new();
    let mut rx_nodes = Vec::new();
    for (i, keypair) in keys(None).into_iter().enumerate() {
        let (tx_output, rx_output) = channel(10);
        let (tx_node_reconfigure, mut rx_node_reconfigure) = channel(10);

        let name = keypair.public().clone();
        let store = NodeStorage::reopen(test_utils::temp_dir());

        let execution_state = Arc::new(SimpleExecutionState::new(
            i,
            committee.clone(),
            tx_node_reconfigure,
        ));
        states.push(execution_state.clone());

        // Start a task that will broadcast the committee change signal.
        let name_clone = name.clone();
        tokio::spawn(async move {
            let mut primary_network = WorkerToPrimaryNetwork::default();
            let mut worker_network = PrimaryToWorkerNetwork::default();

            while let Some((_, committee)) = rx_node_reconfigure.recv().await {
                let address = committee
                    .primary(&name_clone)
                    .expect("Our key is not in the committee")
                    .primary_to_primary;
                let message = WorkerPrimaryMessage::Reconfigure(
                    ReconfigureNotification::NewCommittee(committee.clone()),
                );
                let primary_cancel_handle = primary_network.send(address, &message).await;

                let addresses = committee
                    .our_workers(&name_clone)
                    .expect("Our key is not in the committee")
                    .into_iter()
                    .map(|x| x.primary_to_worker)
                    .collect();
                let message = PrimaryWorkerMessage::Reconfigure(
                    ReconfigureNotification::NewCommittee(committee.clone()),
                );
                let worker_cancel_handles = worker_network.broadcast(addresses, &message).await;

                // Ensure the message has been received.
                primary_cancel_handle.await.unwrap();
                join_all(worker_cancel_handles).await;
            }
        });

        let _primary_handles = Node::spawn_primary(
            keypair,
            Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
            &store,
            parameters.clone(),
            /* consensus */ true,
            execution_state.clone(),
            tx_output,
            &Registry::new(),
        )
        .await
        .unwrap();

        let _worker_handles = Node::spawn_workers(
            name,
            /* worker_ids */ vec![0],
            Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
            &store,
            parameters.clone(),
            &Registry::new(),
        );

        rx_nodes.push(rx_output);
    }

    // Give a chance to the nodes to start.
    tokio::task::yield_now().await;

    // Spawn some clients.
    let mut tx_clients = Vec::new();
    for keypair in keys(None) {
        let (tx_client_reconfigure, rx_client_reconfigure) = channel(10);
        tx_clients.push(tx_client_reconfigure);

        let name = keypair.public().clone();
        let committee = committee.clone();
        tokio::spawn(
            async move { run_client(name, committee.clone(), rx_client_reconfigure).await },
        );
    }

    // Listen to the outputs.
    let mut handles = Vec::new();
    for (tx, mut rx) in tx_clients.into_iter().zip(rx_nodes.into_iter()) {
        handles.push(tokio::spawn(async move {
            let mut current_epoch = 0u64;
            while let Some(output) = rx.recv().await {
                let (outcome, _tx) = output;
                match outcome {
                    Ok(epoch) => {
                        if epoch == 5 {
                            return;
                        }
                        if epoch > current_epoch {
                            current_epoch = epoch;
                            tx.send(current_epoch).await.unwrap();
                        }
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        }));
    }
    join_all(handles).await;
}
