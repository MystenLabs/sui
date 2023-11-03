// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::ConsensusHandlerInitializer;
use crate::consensus_manager::mysticeti_manager::MysticetiManager;
use crate::consensus_manager::narwhal_manager::{
    NarwhalConfiguration, NarwhalManager, NarwhalManagerMetrics,
};
use crate::consensus_validator::SuiTxValidator;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use fastcrypto::traits::KeyPair;
use mysten_metrics::RegistryService;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::{ConsensusConfig, NodeConfig};

pub mod mysticeti_manager;
pub mod narwhal_manager;

/// An enum to easily differentiate between the chosen consensus engine
#[enum_dispatch]
pub enum ConsensusManager {
    Narwhal(NarwhalManager),
    Mysticeti(MysticetiManager),
}

#[async_trait]
#[enum_dispatch(ConsensusManager)]
pub trait ConsensusManagerTrait {
    async fn start(
        &self,
        config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    );

    async fn shutdown(&self);

    fn get_storage_base_path(&self) -> PathBuf;
}

impl ConsensusManager {
    /// Create a new narwhal manager and wrap it around the Manager enum
    pub fn new_narwhal(
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

        Self::Narwhal(NarwhalManager::new(narwhal_config, metrics))
    }
}
