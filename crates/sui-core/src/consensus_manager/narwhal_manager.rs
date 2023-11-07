// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::ConsensusHandlerInitializer;
use crate::consensus_manager::{
    ConsensusManagerMetrics, ConsensusManagerTrait, Running, RunningLockGuard,
};
use crate::consensus_validator::SuiTxValidator;
use async_trait::async_trait;
use fastcrypto::traits::KeyPair;
use mysten_metrics::RegistryService;
use narwhal_config::{Parameters, WorkerId};
use narwhal_network::client::NetworkClient;
use narwhal_node::primary_node::PrimaryNode;
use narwhal_node::worker_node::WorkerNodes;
use narwhal_node::{CertificateStoreCacheMetrics, NodeStorage};
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::NodeConfig;
use sui_types::committee::EpochId;
use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair};
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use tokio::sync::Mutex;

#[cfg(test)]
#[path = "../unit_tests/narwhal_manager_tests.rs"]
pub mod narwhal_manager_tests;

pub struct NarwhalConfiguration {
    pub primary_keypair: AuthorityKeyPair,
    pub network_keypair: NetworkKeyPair,
    pub worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,

    pub storage_base_path: PathBuf,
    pub parameters: Parameters,
    pub registry_service: RegistryService,
}

pub struct NarwhalManager {
    primary_keypair: AuthorityKeyPair,
    network_keypair: NetworkKeyPair,
    worker_ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
    primary_node: PrimaryNode,
    worker_nodes: WorkerNodes,
    storage_base_path: PathBuf,
    running: Mutex<Running>,
    metrics: ConsensusManagerMetrics,
    store_cache_metrics: CertificateStoreCacheMetrics,
}

impl NarwhalManager {
    pub fn new(config: NarwhalConfiguration, metrics: ConsensusManagerMetrics) -> Self {
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

    fn get_store_path(&self, epoch: EpochId) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }
}

#[async_trait]
impl ConsensusManagerTrait for NarwhalManager {
    // Starts the Narwhal (primary & worker(s)) - if not already running.
    // Note: After a binary is updated with the new protocol version and the node
    // is restarted, the protocol config does not take effect until we have a quorum
    // of validators have updated the binary. Because of this the protocol upgrade
    // will happen in the following epoch after quorum is reached. In this case NarwhalManager
    // is not recreated which is why we pass protocol config in at start and not at creation.
    // To ensure correct behavior an updated protocol config must be passed in at the
    // start of EACH epoch.
    async fn start(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        let chain = epoch_store.get_chain_identifier();
        let system_state = epoch_store.epoch_start_state();
        let epoch = epoch_store.epoch();
        let committee = system_state.get_narwhal_committee();
        let protocol_config = epoch_store.protocol_config();

        let Some(_guard) = RunningLockGuard::acquire_start(
            &self.metrics,
            &self.running,
            epoch,
            protocol_config.version,
        )
        .await
        else {
            return;
        };

        let transactions_addr = &config
            .consensus_config
            .as_ref()
            .expect("Validator is missing consensus config")
            .address;
        let worker_cache = system_state.get_narwhal_worker_cache(transactions_addr);

        // Create a new store
        let store_path = self.get_store_path(epoch);
        let store = NodeStorage::reopen(store_path, Some(self.store_cache_metrics.clone()));

        // Create a new client.
        let network_client = NetworkClient::new_from_keypair(&self.network_keypair);

        let name = self.primary_keypair.public().clone();

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
                    narwhal_config::ChainIdentifier::new(*chain.as_bytes()),
                    protocol_config.clone(),
                    worker_cache.clone(),
                    network_client.clone(),
                    &store,
                    consensus_handler_initializer.new_consensus_handler(),
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

        self.metrics
            .start_primary_retries
            .set(primary_retries as i64);
        self.metrics.start_worker_retries.set(worker_retries as i64);
    }

    // Shuts down whole Narwhal (primary & worker(s)) and waits until nodes have shutdown.
    async fn shutdown(&self) {
        let Some(_guard) = RunningLockGuard::acquire_shutdown(&self.metrics, &self.running).await
        else {
            return;
        };

        self.primary_node.shutdown().await;
        self.worker_nodes.shutdown().await;
    }

    async fn is_running(&self) -> bool {
        let running = self.running.lock().await;
        Running::False != *running
    }

    fn get_storage_base_path(&self) -> PathBuf {
        self.storage_base_path.clone()
    }
}
