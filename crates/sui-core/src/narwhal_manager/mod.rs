// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use arc_swap::ArcSwap;
use fastcrypto::bls12381;
use fastcrypto::traits::KeyPair;
use futures::future::join_all;
use narwhal_config::{Committee, Parameters, SharedWorkerCache, WorkerId};
use narwhal_executor::ExecutionState;
use narwhal_node::{Node, NodeStorage};
use narwhal_types::ReconfigureNotification;
use narwhal_worker::TransactionValidator;
use prometheus::Registry;
use std::path::PathBuf;
use std::sync::Arc;
use sui_types::crypto::NetworkKeyPair;
use tokio::task::JoinHandle;

pub struct NarwhalManager {
    primary_handles: Option<Vec<JoinHandle<()>>>,
    worker_handles: Option<Vec<JoinHandle<()>>>,
    parameters: Option<Parameters>,
}

pub struct NarwhalConfiguration<
    State: ExecutionState + Send + Sync + 'static,
    TxValidator: TransactionValidator,
> {
    primary_keypair: bls12381::min_sig::BLS12381KeyPair,
    primary_network_keypair: NetworkKeyPair,

    worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
    worker_cache: SharedWorkerCache,

    committee: Committee,
    storage_base_path: PathBuf,
    registry: Registry,

    parameters: Parameters,
    execution_state: Arc<State>,
    tx_validator: TxValidator,
}

impl NarwhalManager {
    pub fn new() -> Self {
        Self {
            primary_handles: None,
            worker_handles: None,
            parameters: None,
        }
    }

    pub async fn shutdown_narwhal(self) {
        tracing::info!("Sending shutdown message to narwhal");

        // Send shutdown message to the primary, who will forward it to its workers
        match self.parameters {
            Some(parameters) => {
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
            }
            None => {
                tracing::info!("No running instance of narwhal has instantiated yet");
                return;
            }
        }

        // Shutdown the running instance of Narwhal Primary if one is running
        if let Some(handles) = self.primary_handles {
            join_all(handles).await;
        }

        // Shutdown the running instances of Narwhal Workers if they are running
        if let Some(handles) = self.worker_handles {
            join_all(handles).await;
        }

        tracing::info!("Narwhal shutdown is complete");
    }

    pub async fn start_narwhal<State, TxValidator>(
        mut self,
        config: NarwhalConfiguration<State, TxValidator>,
    ) where
        State: ExecutionState + Send + Sync + 'static,
        TxValidator: TransactionValidator,
    {
        // Save parameters for shutdown ability later
        self.parameters = Some(config.parameters.clone());

        // Create a new store
        let mut store_path = config.storage_base_path.clone();
        store_path.push(format!("epoch{}", config.committee.epoch()));
        let store = NodeStorage::reopen(store_path);

        let name = config.primary_keypair.public().clone();

        tracing::info!("Starting up narwhal");

        // Start Narwhal Primary with configuration
        self.primary_handles = Some(
            Node::spawn_primary(
                config.primary_keypair,
                config.primary_network_keypair,
                Arc::new(ArcSwap::new(Arc::new(config.committee.clone()))),
                config.worker_cache.clone(),
                &store,
                config.parameters.clone(),
                /* consensus */ true,
                config.execution_state.clone(),
                &config.registry,
            )
            .await
            .unwrap(),
        );

        // Start Narwhal Workers with configuration
        self.worker_handles = Some(Node::spawn_workers(
            name,
            config.worker_ids_and_keypairs,
            Arc::new(ArcSwap::new(Arc::new(config.committee.clone()))),
            config.worker_cache.clone(),
            &store,
            config.parameters.clone(),
            config.tx_validator.clone(),
            &config.registry,
        ));

        tracing::info!("Starting up narwhal is complete");
    }
}
