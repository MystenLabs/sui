// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{Node, NodeStorage};
use arc_swap::ArcSwap;
use config::{Committee, Parameters, SharedWorkerCache, WorkerCache, WorkerId};
use crypto::{KeyPair, NetworkKeyPair};
use executor::ExecutionState;
use fastcrypto::traits::KeyPair as _;
use futures::future::join_all;
use prometheus::Registry;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc::Receiver;
use types::ReconfigureNotification;
use worker::TransactionValidator;

// Module to start a node (primary, workers and default consensus), keep it running, and restarting it
/// every time the committee changes.
pub struct NodeRestarter;

impl NodeRestarter {
    pub async fn watch<State>(
        primary_keypair: KeyPair,
        primary_network_keypair: NetworkKeyPair,
        worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
        committee: &Committee,
        worker_cache: SharedWorkerCache,
        storage_base_path: PathBuf,
        execution_state: Arc<State>,
        parameters: Parameters,
        tx_validator: impl TransactionValidator,
        mut rx_reconfigure: Receiver<(
            KeyPair,
            NetworkKeyPair,
            Committee,
            Vec<(WorkerId, NetworkKeyPair)>,
            WorkerCache,
        )>,
        registry: &Registry,
    ) where
        State: ExecutionState + Send + Sync + 'static,
    {
        let mut primary_keypair = primary_keypair;
        let mut primary_network_keypair = primary_network_keypair;
        let mut name = primary_keypair.public().clone();
        let mut worker_ids_and_keypairs = worker_ids_and_keypairs;
        let mut committee = committee.clone();

        let mut handles = Vec::new();

        // Listen for new committees.
        loop {
            tracing::info!("Starting epoch E{}", committee.epoch());

            // Get a fresh store for the new epoch.
            let mut store_path = storage_base_path.clone();
            store_path.push(format!("epoch{}", committee.epoch()));
            let store = NodeStorage::reopen(store_path);

            // Restart the relevant components.
            let primary_handles = Node::spawn_primary(
                primary_keypair,
                primary_network_keypair,
                Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
                worker_cache.clone(),
                &store,
                parameters.clone(),
                /* consensus */ true,
                execution_state.clone(),
                registry,
            )
            .await
            .unwrap();

            let worker_handles = Node::spawn_workers(
                name.clone(),
                worker_ids_and_keypairs,
                Arc::new(ArcSwap::new(Arc::new(committee.clone()))),
                worker_cache.clone(),
                &store,
                parameters.clone(),
                tx_validator.clone(),
                registry,
            );

            handles.extend(primary_handles);
            handles.extend(worker_handles);

            // Wait for a committee change.
            let (
                new_keypair,
                new_network_keypair,
                new_committee,
                new_worker_ids_and_keypairs,
                new_worker_cache,
            ) = match rx_reconfigure.recv().await {
                Some(x) => x,
                None => break,
            };
            tracing::info!("Starting reconfiguration with committee {committee}");

            // Shutdown all relevant components.
            // Send shutdown message to the primary, who will forward it to its workers
            let client = reqwest::Client::new();
            client
                .post(format!(
                    "http://127.0.0.1:{}/reconfigure",
                    parameters
                        .network_admin_server
                        .primary_network_admin_server_port
                ))
                .json(&ReconfigureNotification::Shutdown)
                .send()
                .await
                .unwrap();

            tracing::debug!("Committee reconfiguration message successfully sent");

            // Wait for the components to shut down.
            join_all(handles.drain(..)).await;
            tracing::debug!("All tasks successfully exited");

            // Give it an extra second in case the last task to exit is a network server. The OS
            // may need a moment to make the TCP ports available again.
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            tracing::debug!("Epoch E{} terminated", committee.epoch());

            // Update the settings for the next epoch.
            primary_keypair = new_keypair;
            primary_network_keypair = new_network_keypair;
            name = primary_keypair.public().clone();
            worker_ids_and_keypairs = new_worker_ids_and_keypairs;
            committee = new_committee;
            worker_cache.swap(Arc::new(new_worker_cache));
        }
    }
}
