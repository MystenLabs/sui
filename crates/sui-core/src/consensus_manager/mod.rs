// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::{BlockStatusReceiver, ConsensusClient};
use crate::consensus_handler::{
    ConsensusBlockHandler, ConsensusHandlerInitializer, MysticetiConsensusHandler,
};
use crate::consensus_validator::SuiTxValidator;
use crate::mysticeti_adapter::LazyMysticetiClient;
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use consensus_config::{Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use consensus_core::{
    Clock, CommitConsumerArgs, CommitConsumerMonitor, CommitIndex, ConsensusAuthority,
};
use fastcrypto::traits::KeyPair as _;
use mysten_metrics::{RegistryID, RegistryService};
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_config::{ConsensusConfig, NodeConfig};
use sui_protocol_config::{ConsensusNetwork, ProtocolVersion};
use sui_types::error::SuiResult;
use sui_types::messages_consensus::{ConsensusPosition, ConsensusTransaction};
use sui_types::{
    committee::EpochId, sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait,
};
use tokio::sync::{broadcast, Mutex, MutexGuard};
use tokio::time::{sleep, timeout};
use tracing::info;

#[cfg(test)]
#[path = "../unit_tests/consensus_manager_tests.rs"]
pub mod consensus_manager_tests;

#[derive(PartialEq)]
pub(crate) enum Running {
    True(EpochId, ProtocolVersion),
    False,
}

/// Used by Sui validator to start consensus protocol for each epoch.
pub struct ConsensusManager {
    consensus_config: ConsensusConfig,
    protocol_keypair: ProtocolKeyPair,
    network_keypair: NetworkKeyPair,
    storage_base_path: PathBuf,
    metrics: Arc<ConsensusManagerMetrics>,
    registry_service: RegistryService,
    authority: ArcSwapOption<(ConsensusAuthority, RegistryID)>,

    // Use a shared lazy Mysticeti client so we can update the internal Mysticeti
    // client that gets created for every new epoch.
    client: Arc<LazyMysticetiClient>,
    consensus_client: Arc<UpdatableConsensusClient>,

    consensus_handler: Mutex<Option<MysticetiConsensusHandler>>,

    #[cfg(test)]
    pub(crate) consumer_monitor: ArcSwapOption<CommitConsumerMonitor>,
    #[cfg(not(test))]
    consumer_monitor: ArcSwapOption<CommitConsumerMonitor>,
    consumer_monitor_sender: broadcast::Sender<Arc<CommitConsumerMonitor>>,

    running: Mutex<Running>,
    active: parking_lot::Mutex<bool>,

    #[cfg(test)]
    pub(crate) boot_counter: Mutex<u64>,
    #[cfg(not(test))]
    boot_counter: Mutex<u64>,
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
        let client = Arc::new(LazyMysticetiClient::new());
        let (consumer_monitor_sender, _) = broadcast::channel(1);
        Self {
            consensus_config: consensus_config.clone(),
            protocol_keypair: ProtocolKeyPair::new(node_config.worker_key_pair().copy()),
            network_keypair: NetworkKeyPair::new(node_config.network_key_pair().copy()),
            storage_base_path: consensus_config.db_path().to_path_buf(),
            metrics,
            registry_service: registry_service.clone(),
            authority: ArcSwapOption::empty(),
            client,
            consensus_client,
            consensus_handler: Mutex::new(None),
            consumer_monitor: ArcSwapOption::empty(),
            consumer_monitor_sender,
            running: Mutex::new(Running::False),
            active: parking_lot::Mutex::new(false),
            boot_counter: Mutex::new(0),
        }
    }

    pub fn get_storage_base_path(&self) -> PathBuf {
        self.consensus_config.db_path().to_path_buf()
    }

    fn get_store_path(&self, epoch: EpochId) -> PathBuf {
        let mut store_path = self.storage_base_path.clone();
        store_path.push(format!("{}", epoch));
        store_path
    }

    fn pick_network(&self, epoch_store: &AuthorityPerEpochStore) -> ConsensusNetwork {
        if let Ok(type_str) = std::env::var("CONSENSUS_NETWORK") {
            match type_str.to_lowercase().as_str() {
                "anemo" => return ConsensusNetwork::Anemo,
                "tonic" => return ConsensusNetwork::Tonic,
                _ => {
                    info!(
                        "Invalid consensus network type {} in env var. Continue to use the value from protocol config.",
                        type_str
                    );
                }
            }
        }
        epoch_store.protocol_config().consensus_network()
    }
}

impl ConsensusManager {
    pub async fn start(
        &self,
        node_config: &NodeConfig,
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_handler_initializer: ConsensusHandlerInitializer,
        tx_validator: SuiTxValidator,
    ) {
        {
            let mut active = self.active.lock();
            assert!(!*active, "Cannot start consensus. It is already running!");
            info!("Starting consensus ...");
            *active = true;
            self.consensus_client.set(self.client.clone());
        }

        let system_state = epoch_store.epoch_start_state();
        let committee: Committee = system_state.get_consensus_committee();
        let epoch = epoch_store.epoch();
        let protocol_config = epoch_store.protocol_config();
        let network_type = self.pick_network(&epoch_store);

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

        let consensus_config = node_config
            .consensus_config()
            .expect("consensus_config should exist");

        let parameters = Parameters {
            db_path: self.get_store_path(epoch),
            ..consensus_config.parameters.clone().unwrap_or_default()
        };

        let own_protocol_key = self.protocol_keypair.public();
        let (own_index, _) = committee
            .authorities()
            .find(|(_, a)| a.protocol_key == own_protocol_key)
            .expect("Own authority should be among the consensus authorities!");

        let registry = Registry::new_custom(Some("consensus".to_string()), None).unwrap();

        let consensus_handler = consensus_handler_initializer.new_consensus_handler();

        let num_prior_commits = protocol_config.consensus_num_requested_prior_commits_at_startup();
        let last_processed_commit_index =
            consensus_handler.last_processed_subdag_index() as CommitIndex;
        let replay_after_commit_index =
            last_processed_commit_index.saturating_sub(num_prior_commits);

        let (commit_consumer, commit_receiver, block_receiver) =
            CommitConsumerArgs::new(replay_after_commit_index, last_processed_commit_index);
        let monitor = commit_consumer.monitor();

        // Spin up the new Mysticeti consensus handler to listen for committed sub dags, before starting authority.
        let consensus_block_handler = ConsensusBlockHandler::new(
            epoch_store.clone(),
            consensus_handler.execution_scheduler_sender().clone(),
            consensus_handler_initializer.backpressure_subscriber(),
            consensus_handler_initializer.metrics().clone(),
        );
        let handler = MysticetiConsensusHandler::new(
            last_processed_commit_index,
            consensus_handler,
            consensus_block_handler,
            commit_receiver,
            block_receiver,
            monitor.clone(),
        );
        let mut consensus_handler = self.consensus_handler.lock().await;
        *consensus_handler = Some(handler);

        // If there is a previous consumer monitor, it indicates that the consensus engine has been restarted, due to an epoch change. However, that on its
        // own doesn't tell us much whether it participated on an active epoch or an old one. We need to check if it has handled any commits to determine this.
        // If indeed any commits did happen, then we assume that node did participate on previous run.
        let participated_on_previous_run =
            if let Some(previous_monitor) = self.consumer_monitor.swap(Some(monitor.clone())) {
                previous_monitor.highest_handled_commit() > 0
            } else {
                false
            };

        // Increment the boot counter only if the consensus successfully participated in the previous run.
        // This is typical during normal epoch changes, where the node restarts as expected, and the boot counter is incremented to prevent amnesia recovery on the next start.
        // If the node is recovering from a restore process and catching up across multiple epochs, it won't handle any commits until it reaches the last active epoch.
        // In this scenario, we do not increment the boot counter, as we need amnesia recovery to run.
        let mut boot_counter = self.boot_counter.lock().await;
        if participated_on_previous_run {
            *boot_counter += 1;
        } else {
            info!(
                "Node has not participated in previous epoch consensus. Boot counter ({}) will not increment.",
                *boot_counter
            );
        }

        let authority = ConsensusAuthority::start(
            network_type,
            epoch_store.epoch_start_config().epoch_start_timestamp_ms(),
            own_index,
            committee.clone(),
            parameters.clone(),
            protocol_config.clone(),
            self.protocol_keypair.clone(),
            self.network_keypair.clone(),
            Arc::new(Clock::default()),
            Arc::new(tx_validator.clone()),
            commit_consumer,
            registry.clone(),
            *boot_counter,
        )
        .await;
        let client = authority.transaction_client();

        let registry_id = self.registry_service.add(registry.clone());

        let registered_authority = Arc::new((authority, registry_id));
        self.authority.swap(Some(registered_authority.clone()));

        // Initialize the client to send transactions to this Mysticeti instance.
        self.client.set(client);

        // Send the consumer monitor to the replay waiter.
        let _ = self.consumer_monitor_sender.send(monitor);
    }

    pub async fn shutdown(&self) {
        info!("Shutting down consensus ...");
        let prev_active = {
            let mut active = self.active.lock();
            std::mem::replace(&mut *active, false)
        };
        if !prev_active {
            return;
        }

        let Some(_guard) = RunningLockGuard::acquire_shutdown(&self.metrics, &self.running).await
        else {
            return;
        };

        // Stop consensus submissions.
        self.client.clear();

        // swap with empty to ensure there is no other reference to authority and we can safely do Arc unwrap
        let r = self.authority.swap(None).unwrap();
        let Ok((authority, registry_id)) = Arc::try_unwrap(r) else {
            panic!("Failed to retrieve the Mysticeti authority");
        };

        // shutdown the authority and wait for it
        authority.stop().await;

        // drop the old consensus handler to force stop any underlying task running.
        let mut consensus_handler = self.consensus_handler.lock().await;
        if let Some(mut handler) = consensus_handler.take() {
            handler.abort().await;
        }

        // unregister the registry id
        self.registry_service.remove(registry_id);

        self.consensus_client.clear();
    }

    pub async fn is_running(&self) -> bool {
        let active = self.active.lock();
        *active
    }

    pub fn replay_waiter(&self) -> ReplayWaiter {
        let consumer_monitor_receiver = self.consumer_monitor_sender.subscribe();
        ReplayWaiter::new(consumer_monitor_receiver)
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
    ) -> SuiResult<(Vec<ConsensusPosition>, BlockStatusReceiver)> {
        let client = self.get().await;
        client.submit(transactions, epoch_store).await
    }
}

/// Waits for consensus to finish replaying at consensus handler.
pub struct ReplayWaiter {
    consumer_monitor_receiver: broadcast::Receiver<Arc<CommitConsumerMonitor>>,
}

impl ReplayWaiter {
    pub(crate) fn new(
        consumer_monitor_receiver: broadcast::Receiver<Arc<CommitConsumerMonitor>>,
    ) -> Self {
        Self {
            consumer_monitor_receiver,
        }
    }

    pub(crate) async fn wait_for_replay(mut self) {
        loop {
            info!("Waiting for consensus to start replaying ...");
            let Ok(monitor) = self.consumer_monitor_receiver.recv().await else {
                continue;
            };
            info!("Waiting for consensus handler to finish replaying ...");
            monitor
                .replay_to_consumer_last_processed_commit_complete()
                .await;
            break;
        }
    }
}

impl Clone for ReplayWaiter {
    fn clone(&self) -> Self {
        Self {
            consumer_monitor_receiver: self.consumer_monitor_receiver.resubscribe(),
        }
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
                "Starting up consensus for epoch {} & protocol version {:?} completed - took {} seconds",
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
