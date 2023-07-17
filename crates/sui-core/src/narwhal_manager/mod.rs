// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
#[path = "../unit_tests/narwhal_manager_tests.rs"]
pub mod narwhal_manager_tests;

use fastcrypto::traits::KeyPair;
use mysten_metrics::RegistryService;
use narwhal_config::{Committee, Epoch, Parameters, WorkerCache, WorkerId};
use narwhal_executor::ExecutionState;
use narwhal_network::client::NetworkClient;
use narwhal_node::primary_node::PrimaryNode;
use narwhal_node::worker_node::WorkerNodes;
use narwhal_node::{CertificateStoreCacheMetrics, NodeStorage};
use narwhal_worker::TransactionValidator;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair};
use tokio::sync::Mutex;

#[derive(PartialEq)]
enum Running {
    True(Epoch, ProtocolVersion),
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

pub struct NarwhalManagerMetrics {
    start_latency: IntGauge,
    shutdown_latency: IntGauge,
    start_primary_retries: IntGauge,
    start_worker_retries: IntGauge,
}

impl NarwhalManagerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            start_latency: register_int_gauge_with_registry!(
                "narwhal_manager_start_latency",
                "The latency of starting up narwhal nodes",
                registry,
            )
            .unwrap(),
            shutdown_latency: register_int_gauge_with_registry!(
                "narwhal_manager_shutdown_latency",
                "The latency of shutting down narwhal nodes",
                registry,
            )
            .unwrap(),
            start_primary_retries: register_int_gauge_with_registry!(
                "narwhal_manager_start_primary_retries",
                "The number of retries took to start narwhal primary node",
                registry
            )
            .unwrap(),
            start_worker_retries: register_int_gauge_with_registry!(
                "narwhal_manager_start_worker_retries",
                "The number of retries took to start narwhal worker node",
                registry
            )
            .unwrap(),
        }
    }
}

pub struct NarwhalManager {
    primary_keypair: AuthorityKeyPair,
    network_keypair: NetworkKeyPair,
    worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
    primary_node: PrimaryNode,
    worker_nodes: WorkerNodes,
    storage_base_path: PathBuf,
    running: Mutex<Running>,
    metrics: NarwhalManagerMetrics,
    store_cache_metrics: CertificateStoreCacheMetrics,
}

impl NarwhalManager {
    pub fn new(config: NarwhalConfiguration, metrics: NarwhalManagerMetrics) -> Self {
        // Create the Narwhal Primary with configuration
        let primary_node =
            PrimaryNode::new(config.parameters.clone(), config.registry_service.clone());

        // Create Narwhal Workers with configuration
        let worker_nodes =
            WorkerNodes::new(config.registry_service.clone(), config.parameters.clone());

        let store_cache_metrics =
            CertificateStoreCacheMetrics::new(&config.registry_service.default_registry());

        Self {
            primary_node,
            worker_nodes,
            primary_keypair: config.primary_keypair,
            network_keypair: config.network_keypair,
            worker_ids_and_keypairs: config.worker_ids_and_keypairs,
            storage_base_path: config.storage_base_path,
            running: Mutex::new(Running::False),
            metrics,
            store_cache_metrics,
        }
    }

    // Starts the Narwhal (primary & worker(s)) - if not already running.
    // Note: After a binary is updated with the new protocol version and the node
    // is restarted, the protocol config does not take effect until we have a quorum
    // of validators have updated the binary. Because of this the protocol upgrade
    // will happen in the following epoch after quorum is reached. In this case NarwhalManager
    // is not recreated which is why we pass protocol config in at start and not at creation.
    // To ensure correct behavior an updated protocol config must be passed in at the
    // start of EACH epoch.
    pub async fn start<State, TxValidator: TransactionValidator>(
        &self,
        committee: Committee,
        protocol_config: ProtocolConfig,
        worker_cache: WorkerCache,
        execution_state: Arc<State>,
        tx_validator: TxValidator,
    ) where
        State: ExecutionState + Send + Sync + 'static,
    {
        let mut running = self.running.lock().await;

        if let Running::True(epoch, version) = *running {
            tracing::warn!(
                "Narwhal node is already Running for epoch {epoch:?} & protocol version {version:?} - shutdown first before starting",
            );
            return;
        }

        let now = Instant::now();

        // Create a new store
        let store_path = self.get_store_path(committee.epoch());
        let store = NodeStorage::reopen(store_path, Some(self.store_cache_metrics.clone()));

        // Create a new client.
        let network_client = NetworkClient::new_from_keypair(&self.network_keypair);

        let name = self.primary_keypair.public().clone();

        tracing::info!(
            "Starting up Narwhal for epoch {} & protocol version {:?}",
            committee.epoch(),
            protocol_config.version
        );

        // start primary
        const MAX_PRIMARY_RETRIES: u32 = 2;
        let mut primary_retries = 0;
        loop {
            match self
                .primary_node
                .start(
                    self.primary_keypair.copy(),
                    self.network_keypair.copy(),
                    committee.clone(),
                    protocol_config.clone(),
                    worker_cache.clone(),
                    network_client.clone(),
                    &store,
                    execution_state.clone(),
                )
                .await
            {
                Ok(_) => {
                    break;
                }
                Err(e) => {
                    primary_retries += 1;
                    if primary_retries >= MAX_PRIMARY_RETRIES {
                        panic!("Unable to start Narwhal Primary: {:?}", e);
                    }
                    tracing::error!("Unable to start Narwhal Primary: {:?}, retrying", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }

        // Start Narwhal Workers with configuration
        const MAX_WORKER_RETRIES: u32 = 2;
        let mut worker_retries = 0;
        loop {
            // Copy the config for this iteration of the loop
            let id_keypair_copy = self
                .worker_ids_and_keypairs
                .iter()
                .map(|(id, keypair)| (*id, keypair.copy()))
                .collect();

            match self
                .worker_nodes
                .start(
                    name.clone(),
                    id_keypair_copy,
                    committee.clone(),
                    protocol_config.clone(),
                    worker_cache.clone(),
                    network_client.clone(),
                    &store,
                    tx_validator.clone(),
                )
                .await
            {
                Ok(_) => {
                    break;
                }
                Err(e) => {
                    worker_retries += 1;
                    if worker_retries >= MAX_WORKER_RETRIES {
                        panic!("Unable to start Narwhal Worker: {:?}", e);
                    }
                    tracing::error!("Unable to start Narwhal Worker: {:?}, retrying", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }

        tracing::info!(
            "Starting up Narwhal for epoch {} & protocol version {:?} is complete - took {} seconds",
            committee.epoch(),
            protocol_config.version,
            now.elapsed().as_secs_f64()
        );

        self.metrics
            .start_latency
            .set(now.elapsed().as_secs_f64() as i64);

        self.metrics
            .start_primary_retries
            .set(primary_retries as i64);
        self.metrics.start_worker_retries.set(worker_retries as i64);

        *running = Running::True(committee.epoch(), protocol_config.version);
    }

    // Shuts down whole Narwhal (primary & worker(s)) and waits until nodes
    // have shutdown.
    pub async fn shutdown(&self) {
        let mut running = self.running.lock().await;

        match *running {
            Running::True(epoch, version) => {
                let now = Instant::now();
                tracing::info!(
                    "Shutting down Narwhal for epoch {epoch:?} & protocol version {version:?}"
                );

                self.primary_node.shutdown().await;
                self.worker_nodes.shutdown().await;

                tracing::info!(
                    "Narwhal shutdown for epoch {epoch:?} & protocol version {version:?} is complete - took {} seconds",
                    now.elapsed().as_secs_f64()
                );

                self.metrics
                    .shutdown_latency
                    .set(now.elapsed().as_secs_f64() as i64);
            }
            Running::False => {
                tracing::info!(
                    "Narwhal Manager shutdown was called but Narwhal node is not running"
                );
            }
        }

        *running = Running::False;
    }

    fn get_store_path(&self, epoch: Epoch) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }

    pub fn get_storage_base_path(&self) -> PathBuf {
        self.storage_base_path.clone()
    }
}
