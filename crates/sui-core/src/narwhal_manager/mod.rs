// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
#[path = "../unit_tests/narwhal_manager_tests.rs"]
pub mod narwhal_manager_tests;

use arc_swap::ArcSwap;
use fastcrypto::traits::KeyPair;
use mysten_metrics::{monitored_scope, spawn_monitored_task, RegistryService};
use narwhal_config::{Committee, Epoch, Parameters, SharedWorkerCache, WorkerId};
use narwhal_executor::ExecutionState;
use narwhal_node::primary_node::PrimaryNode;
use narwhal_node::worker_node::WorkerNodes;
use narwhal_node::NodeStorage;
use narwhal_worker::TransactionValidator;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair};
use tokio::sync::Mutex;

#[derive(PartialEq)]
enum Running {
    True(Epoch),
    False,
}

pub struct NarwhalConfiguration {
    pub primary_keypair: AuthorityKeyPair,
    pub network_keypair: NetworkKeyPair,
    pub worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,

    pub storage_base_path: PathBuf,
    pub parameters: Parameters,
    pub registry_service: RegistryService,
}

pub struct NarwhalManager {
    storage_base_path: PathBuf,
    primary_keypair: AuthorityKeyPair,
    network_keypair: NetworkKeyPair,
    worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
    primary_node: PrimaryNode,
    worker_nodes: WorkerNodes,
    running: Mutex<Running>,
}

impl NarwhalManager {
    pub fn new(config: NarwhalConfiguration) -> Self {
        // Create the Narwhal Primary with configuration
        let primary_node = PrimaryNode::new(
            config.parameters.clone(),
            true,
            config.registry_service.clone(),
        );

        // Create Narwhal Workers with configuration
        let worker_nodes =
            WorkerNodes::new(config.registry_service.clone(), config.parameters.clone());

        Self {
            primary_node,
            worker_nodes,
            primary_keypair: config.primary_keypair,
            network_keypair: config.network_keypair,
            worker_ids_and_keypairs: config.worker_ids_and_keypairs,
            storage_base_path: config.storage_base_path,
            running: Mutex::new(Running::False),
        }
    }

    // Starts the Narwhal (primary & worker(s)) - if not already running.
    pub async fn start<State, TxValidator: TransactionValidator>(
        &self,
        committee: Arc<Committee>,
        shared_worker_cache: SharedWorkerCache,
        execution_state: Arc<State>,
        tx_validator: TxValidator,
    ) where
        State: ExecutionState + Send + Sync + 'static,
    {
        let mut running = self.running.lock().await;

        if let Running::True(epoch) = *running {
            tracing::warn!(
                "Narwhal node is already Running at epoch {:?} - shutdown first before starting",
                epoch
            );
            return;
        }

        let _guard = monitored_scope("NarwhalManagerStart");

        // Create a new store
        let store_path = self.get_store_path(committee.epoch());
        let store = NodeStorage::reopen(store_path);

        let name = self.primary_keypair.public().clone();

        let now = Instant::now();
        tracing::info!("Starting up Narwhal for epoch {}", committee.epoch());

        // start primary
        self.primary_node
            .start(
                self.primary_keypair.copy(),
                self.network_keypair.copy(),
                Arc::new(ArcSwap::new(committee.clone())),
                shared_worker_cache.clone(),
                &store,
                execution_state,
            )
            .await
            .expect("Unable to start Narwhal Primary");

        // Start Narwhal Workers with configuration
        // Copy the config for this iteration of the loop
        let id_keypair_copy = self
            .worker_ids_and_keypairs
            .iter()
            .map(|(id, keypair)| (*id, keypair.copy()))
            .collect();

        self.worker_nodes
            .start(
                name,
                id_keypair_copy,
                Arc::new(ArcSwap::new(committee.clone())),
                shared_worker_cache,
                &store,
                tx_validator.clone(),
            )
            .await
            .expect("Unable to start Narwhal Worker");

        tracing::info!(
            "Starting up Narwhal for epoch {} is complete - took {} seconds",
            committee.epoch(),
            now.elapsed().as_secs_f64()
        );

        *running = Running::True(committee.epoch());
    }

    // Shuts down whole Narwhal (primary & worker(s)) and waits until nodes
    // have shutdown.
    pub async fn shutdown(&self) {
        let mut running = self.running.lock().await;

        match *running {
            Running::True(epoch) => {
                let _guard = monitored_scope("NarwhalManagerShutdown");

                let now = Instant::now();
                tracing::info!("Shutting down Narwhal");

                self.primary_node.shutdown().await;
                self.worker_nodes.shutdown().await;

                tracing::info!(
                    "Narwhal shutdown is complete - took {} seconds",
                    now.elapsed().as_secs_f64()
                );

                // Drop storage
                let path_clone = self.storage_base_path.clone();
                spawn_monitored_task!(Self::remove_old_epoch_data(path_clone, epoch));
            }
            Running::False => {
                tracing::info!("Shutdown was called but Narwhal node is not running");
            }
        }

        *running = Running::False;
    }

    fn get_store_path(&self, epoch: Epoch) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }

    async fn remove_old_epoch_data(storage_base_path: PathBuf, epoch: Epoch) {
        // Keep previous epoch data as a safety buffer and remove starting from epoch - 1
        let drop_boundary = epoch - 1;

        // Get all the epoch stores in the base path directory
        let files = match fs::read_dir(storage_base_path.clone()) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("Narwhal Manager cannot read the files in the storage path directory for epoch cleanup: {:?}", e);
                return;
            }
        };

        // Look for any that are less than or equal to the drop boundary and drop
        for file_res in files {
            let f = match file_res {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!(
                        "Narwhal Manager error while cleaning up storage of previous epochs: {:?}",
                        e
                    );
                    continue;
                }
            };

            let file_epoch_string = f.file_name().to_str().unwrap().to_owned(); // todo:remove unwrap
            let file_epoch = match file_epoch_string.parse::<u64>() {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("Narwhal Manager could not parse file in storage path into epoch for cleanup: {:?}",e);
                    continue;
                }
            };

            if file_epoch <= drop_boundary {
                if let Err(e) = fs::remove_dir(f.path()) {
                    tracing::error!(
                        "Narwhal Manager could not remove old epoch storage directory: {:?}",
                        e
                    );
                }
            }
        }
    }
}
