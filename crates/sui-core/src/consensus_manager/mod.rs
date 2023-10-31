// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_manager::mysticeti_manager::MysticetiManager;
use crate::consensus_manager::narwhal_manager::{
    NarwhalConfiguration, NarwhalManager, NarwhalManagerMetrics,
};
use fastcrypto::traits::KeyPair;
use mysten_metrics::RegistryService;
use narwhal_executor::ExecutionState;
use narwhal_worker::TransactionValidator;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::{ConsensusConfig, NodeConfig};

pub mod mysticeti_manager;
pub mod narwhal_manager;

/// An enum to easily differentiate between the chosen consensus engine
pub enum Manager {
    Narwhal(NarwhalManager),
    Mysticeti(MysticetiManager),
}

impl Manager {
    /// Create a new narwhal manager and wrap it around the Manager enum
    pub fn narwhal(
        config: &NodeConfig,
        consensus_config: &ConsensusConfig,
        registry_service: &RegistryService,
    ) -> Self {
        let narwhal_config = NarwhalConfiguration {
            primary_keypair: config.protocol_key_pair().copy(),
            network_keypair: config.network_key_pair().copy(),
            worker_ids_and_keypairs: vec![(0, config.worker_key_pair().copy())],
            storage_base_path: consensus_config.db_path().to_path_buf(),
            parameters: consensus_config.narwhal_config().to_owned(),
            registry_service: registry_service.clone(),
        };

        let metrics = NarwhalManagerMetrics::new(&registry_service.default_registry());

        Manager::Narwhal(NarwhalManager::new(narwhal_config, metrics))
    }

    // Starts the underneath consensus manager by the given inputs
    pub async fn start<State, StateInitializer, TxValidator: TransactionValidator>(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        execution_state: StateInitializer,
        tx_validator: TxValidator,
    ) where
        State: ExecutionState + Send + Sync + 'static,
        StateInitializer: Fn() -> State + Send + Sync,
    {
        match self {
            Manager::Narwhal(narwhal_manager) => {
                narwhal_manager
                    .start(config, epoch_store, execution_state, tx_validator)
                    .await
            }
            Manager::Mysticeti(mysticeti_manager) => {
                mysticeti_manager
                    .start(config, epoch_store, execution_state, tx_validator)
                    .await
            }
        }
    }

    // Shutting down the underneath consensus manager
    pub async fn shutdown(&self) {
        match self {
            Manager::Narwhal(manager) => manager.shutdown().await,
            Manager::Mysticeti(manager) => manager.shutdown().await,
        }
    }

    pub fn get_storage_base_path(&self) -> PathBuf {
        match self {
            Manager::Narwhal(manager) => manager.get_storage_base_path(),
            Manager::Mysticeti(manager) => manager.get_storage_base_path(),
        }
    }
}
