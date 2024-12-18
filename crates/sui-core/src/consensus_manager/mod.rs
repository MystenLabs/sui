// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::{BlockStatusReceiver, ConsensusClient};
use crate::consensus_handler::ConsensusHandlerInitializer;
use crate::consensus_manager::mysticeti_manager::MysticetiManager;
use crate::consensus_validator::SuiTxValidator;
use crate::mysticeti_adapter::LazyMysticetiClient;
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use fastcrypto::traits::KeyPair as _;
use mysten_metrics::RegistryService;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_config::{ConsensusConfig, NodeConfig};
use sui_protocol_config::ProtocolVersion;
use sui_types::committee::EpochId;
use sui_types::error::SuiResult;
use sui_types::messages_consensus::ConsensusTransaction;
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::{sleep, timeout};
use tracing::info;

pub mod mysticeti_manager;

#[derive(PartialEq)]
pub(crate) enum Running {
    True(EpochId, ProtocolVersion),
    False,
}

#[async_trait]
#[enum_dispatch(ProtocolManager)]
pub trait ConsensusManagerTrait {
    async fn start(
        &self,
        node_config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    );

    async fn shutdown(&self);

    async fn is_running(&self) -> bool;
}

// Wraps the underlying consensus protocol managers to make calling
// the ConsensusManagerTrait easier.
#[enum_dispatch]
enum ProtocolManager {
    Mysticeti(MysticetiManager),
}

impl ProtocolManager {
    /// Creates a new mysticeti manager.
    pub fn new_mysticeti(
        config: &NodeConfig,
        consensus_config: &ConsensusConfig,
        registry_service: &RegistryService,
        metrics: Arc<ConsensusManagerMetrics>,
        client: Arc<LazyMysticetiClient>,
    ) -> Self {
        Self::Mysticeti(MysticetiManager::new(
            config.worker_key_pair().copy(),
            config.network_key_pair().copy(),
            consensus_config.db_path().to_path_buf(),
            registry_service.clone(),
            metrics,
            client,
        ))
    }
}

/// Used by Sui validator to start consensus protocol for each epoch.
pub struct ConsensusManager {
    consensus_config: ConsensusConfig,
    mysticeti_manager: ProtocolManager,
    mysticeti_client: Arc<LazyMysticetiClient>,
    active: parking_lot::Mutex<bool>,
    consensus_client: Arc<UpdatableConsensusClient>,
}

impl ConsensusManager {
    pub fn new(
        node_config: &NodeConfig,
        consensus_config: &ConsensusConfig,
        registry_service: &RegistryService,
        consensus_client: Arc<UpdatableConsensusClient>,
    ) -> Self {
        let metrics = Arc::new(ConsensusManagerMetrics::new(
            &registry_service.default_registry(),
        ));
        let mysticeti_client = Arc::new(LazyMysticetiClient::new());
        let mysticeti_manager = ProtocolManager::new_mysticeti(
            node_config,
            consensus_config,
            registry_service,
            metrics,
            mysticeti_client.clone(),
        );
        Self {
            consensus_config: consensus_config.clone(),
            mysticeti_manager,
            mysticeti_client,
            active: parking_lot::Mutex::new(false),
            consensus_client,
        }
    }

    pub fn get_storage_base_path(&self) -> PathBuf {
        self.consensus_config.db_path().to_path_buf()
    }
}

#[async_trait]
impl ConsensusManagerTrait for ConsensusManager {
    async fn start(
        &self,
        node_config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        let protocol_manager = {
            let mut active = self.active.lock();
            assert!(!*active, "Cannot start consensus. It is already running!");
            info!("Starting consensus ...");
            *active = true;
            self.consensus_client.set(self.mysticeti_client.clone());
            &self.mysticeti_manager
        };

        protocol_manager
            .start(
                node_config,
                epoch_store,
                consensus_handler_initializer,
                tx_validator,
            )
            .await
    }

    async fn shutdown(&self) {
        info!("Shutting down consensus ...");
        let prev_active = {
            let mut active = self.active.lock();
            std::mem::replace(&mut *active, false)
        };
        if prev_active {
            self.mysticeti_manager.shutdown().await;
        }
        self.consensus_client.clear();
    }

    async fn is_running(&self) -> bool {
        let active = self.active.lock();
        *active
    }
}

/// A ConsensusClient that can be updated internally at any time. This usually happening during epoch
/// change where a client is set after the new consensus is started for the new epoch.
#[derive(Default)]
pub struct UpdatableConsensusClient {
    // An extra layer of Arc<> is needed as required by ArcSwapAny.
    client: ArcSwapOption<Arc<dyn ConsensusClient>>,
}

impl UpdatableConsensusClient {
    pub fn new() -> Self {
        Self {
            client: ArcSwapOption::empty(),
        }
    }

    async fn get(&self) -> Arc<Arc<dyn ConsensusClient>> {
        const START_TIMEOUT: Duration = Duration::from_secs(30);
        const RETRY_INTERVAL: Duration = Duration::from_millis(100);
        if let Ok(client) = timeout(START_TIMEOUT, async {
            loop {
                let Some(client) = self.client.load_full() else {
                    sleep(RETRY_INTERVAL).await;
                    continue;
                };
                return client;
            }
        })
        .await
        {
            return client;
        }

        panic!(
            "Timed out after {:?} waiting for Consensus to start!",
            START_TIMEOUT,
        );
    }

    pub fn set(&self, client: Arc<dyn ConsensusClient>) {
        self.client.store(Some(Arc::new(client)));
    }

    pub fn clear(&self) {
        self.client.store(None);
    }
}

#[async_trait]
impl ConsensusClient for UpdatableConsensusClient {
    async fn submit(
        &self,
        transactions: &[ConsensusTransaction],
        epoch_store: &Arc<AuthorityPerEpochStore>,
    ) -> SuiResult<BlockStatusReceiver> {
        let client = self.get().await;
        client.submit(transactions, epoch_store).await
    }
}

pub struct ConsensusManagerMetrics {
    start_latency: IntGauge,
    shutdown_latency: IntGauge,
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
            tracing::warn!("Consensus shutdown was called but consensus is not running");
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
