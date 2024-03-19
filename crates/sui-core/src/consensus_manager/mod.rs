// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_handler::ConsensusHandlerInitializer;
use crate::consensus_manager::mysticeti_manager::MysticetiManager;
use crate::consensus_manager::narwhal_manager::{NarwhalConfiguration, NarwhalManager};
use crate::consensus_validator::SuiTxValidator;
use crate::mysticeti_adapter::LazyMysticetiClient;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use fastcrypto::traits::KeyPair;
use mysten_metrics::RegistryService;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use sui_config::{ConsensusConfig, NodeConfig};
use sui_protocol_config::ProtocolVersion;
use sui_types::committee::EpochId;
use tokio::sync::{Mutex, MutexGuard};

pub mod mysticeti_manager;
pub mod narwhal_manager;

#[derive(PartialEq)]
pub(crate) enum Running {
    True(EpochId, ProtocolVersion),
    False,
}

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

    async fn is_running(&self) -> bool;

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

        let metrics = ConsensusManagerMetrics::new(&registry_service.default_registry());

        Self::Narwhal(NarwhalManager::new(narwhal_config, metrics))
    }

    pub fn new_mysticeti(
        config: &NodeConfig,
        consensus_config: &ConsensusConfig,
        registry_service: &RegistryService,
        client: Arc<LazyMysticetiClient>,
    ) -> Self {
        let metrics = ConsensusManagerMetrics::new(&registry_service.default_registry());

        Self::Mysticeti(MysticetiManager::new(
            config.protocol_key_pair().copy(),
            config.network_key_pair().copy(),
            consensus_config.db_path().to_path_buf(),
            metrics,
            registry_service.clone(),
            client,
        ))
    }
}

pub struct ConsensusManagerMetrics {
    start_latency: IntGauge,
    shutdown_latency: IntGauge,
    start_primary_retries: IntGauge,
    start_worker_retries: IntGauge,
}

impl ConsensusManagerMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            start_latency: register_int_gauge_with_registry!(
                "consensus_manager_start_latency",
                "The latency of starting up consensus nodes",
                registry,
            )
            .unwrap(),
            shutdown_latency: register_int_gauge_with_registry!(
                "consensus_manager_shutdown_latency",
                "The latency of shutting down consensus nodes",
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

pub(crate) struct RunningLockGuard<'a> {
    state_guard: MutexGuard<'a, Running>,
    metrics: &'a ConsensusManagerMetrics,
    epoch: Option<EpochId>,
    protocol_version: Option<ProtocolVersion>,
    start: Instant,
}

impl<'a> RunningLockGuard<'a> {
    pub(crate) async fn acquire_start(
        metrics: &'a ConsensusManagerMetrics,
        running_mutex: &'a Mutex<Running>,
        epoch: EpochId,
        version: ProtocolVersion,
    ) -> Option<RunningLockGuard<'a>> {
        let running = running_mutex.lock().await;
        if let Running::True(epoch, version) = *running {
            tracing::warn!(
                "Consensus is already Running for epoch {epoch:?} & protocol version {version:?} - shutdown first before starting",
            );
            return None;
        }

        tracing::info!("Starting up consensus for epoch {epoch:?} & protocol version {version:?}");

        Some(RunningLockGuard {
            state_guard: running,
            metrics,
            start: Instant::now(),
            epoch: Some(epoch),
            protocol_version: Some(version),
        })
    }

    pub(crate) async fn acquire_shutdown(
        metrics: &'a ConsensusManagerMetrics,
        running_mutex: &'a Mutex<Running>,
    ) -> Option<RunningLockGuard<'a>> {
        let running = running_mutex.lock().await;
        if let Running::True(epoch, version) = *running {
            tracing::info!(
                "Shutting down consensus for epoch {epoch:?} & protocol version {version:?}"
            );
        } else {
            tracing::warn!("Consensus shutdown was called but Narwhal node is not running");
            return None;
        }

        Some(RunningLockGuard {
            state_guard: running,
            metrics,
            start: Instant::now(),
            epoch: None,
            protocol_version: None,
        })
    }
}

impl Drop for RunningLockGuard<'_> {
    fn drop(&mut self) {
        match *self.state_guard {
            // consensus was running and now will have to be marked as shutdown
            Running::True(epoch, version) => {
                tracing::info!("Consensus shutdown for epoch {epoch:?} & protocol version {version:?} is complete - took {} seconds", self.start.elapsed().as_secs_f64());

                self.metrics
                    .shutdown_latency
                    .set(self.start.elapsed().as_secs_f64() as i64);

                *self.state_guard = Running::False;
            }
            // consensus was not running and now will be marked as started
            Running::False => {
                tracing::info!(
                "Starting up consensus for epoch {} & protocol version {:?} is complete - took {} seconds",
                self.epoch.unwrap(),
                self.protocol_version.unwrap(),
                self.start.elapsed().as_secs_f64());

                self.metrics
                    .start_latency
                    .set(self.start.elapsed().as_secs_f64() as i64);

                *self.state_guard =
                    Running::True(self.epoch.unwrap(), self.protocol_version.unwrap());
            }
        }
    }
}
