// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{temp_dir, CommitteeFixture};
use arc_swap::ArcSwap;
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use crypto::{KeyPair, NetworkKeyPair, PublicKey};
use executor::SerializedTransaction;
use fastcrypto::traits::KeyPair as _;
use itertools::Itertools;
use multiaddr::Multiaddr;
use node::{
    execution_state::SimpleExecutionState,
    metrics::{primary_metrics_registry, worker_metrics_registry},
    Node,
};
use prometheus::{proto::Metric, Registry};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, sync::Arc, time::Duration};
use storage::NodeStorage;
use telemetry_subscribers::TelemetryGuards;
use tokio::{
    sync::{broadcast::Sender, mpsc::channel, RwLock},
    task::JoinHandle,
};
use tonic::transport::Channel;
use tracing::info;
use types::{ConfigurationClient, ProposerClient, TransactionsClient};
use worker::TrivialTransactionValidator;

#[cfg(test)]
#[path = "tests/cluster_tests.rs"]
pub mod cluster_tests;

pub struct Cluster {
    #[allow(unused)]
    fixture: CommitteeFixture,
    authorities: HashMap<usize, AuthorityDetails>,
    pub committee_shared: SharedCommittee,
    pub worker_cache_shared: SharedWorkerCache,
    #[allow(dead_code)]
    parameters: Parameters,
}

impl Cluster {
    /// Initialises a new cluster by the provided parameters. The cluster will
    /// create all the authorities (primaries & workers) that are defined under
    /// the committee structure, but none of them will be started.
    ///
    /// When the `internal_consensus_enabled` is true then the standard internal
    /// consensus engine will be enabled. If false, then the internal consensus will
    /// be disabled and the gRPC server will be enabled to manage the Collections & the
    /// DAG externally.
    pub fn new(parameters: Option<Parameters>, internal_consensus_enabled: bool) -> Self {
        let fixture = CommitteeFixture::builder().randomize_ports(true).build();
        let c = fixture.committee();
        let shared_worker_cache = fixture.shared_worker_cache();
        let shared_committee = Arc::new(ArcSwap::from_pointee(c));
        let params = parameters.unwrap_or_else(Self::parameters);

        info!("###### Creating new cluster ######");
        info!("Validator keys:");
        let mut nodes = HashMap::new();

        for (id, authority_fixture) in fixture.authorities().enumerate() {
            info!("Key {id} -> {}", authority_fixture.public_key());

            let authority = AuthorityDetails::new(
                id,
                authority_fixture.keypair().copy(),
                authority_fixture.network_keypair().copy(),
                authority_fixture.worker_keypairs(),
                params.clone(),
                shared_committee.clone(),
                shared_worker_cache.clone(),
                internal_consensus_enabled,
            );
            nodes.insert(id, authority);
        }

        Self {
            fixture,
            authorities: nodes,
            committee_shared: shared_committee,
            worker_cache_shared: shared_worker_cache,
            parameters: params,
        }
    }

    /// Starts a cluster by the defined number of authorities. The authorities
    /// will be started sequentially started from the one with id zero up to
    /// the provided number `authorities_number`. If none number is provided, then
    /// the maximum number of authorities will be started.
    /// If a number higher than the available ones in the committee is provided then
    /// the method will panic.
    /// The workers_per_authority dictates how many workers per authority should
    /// also be started (the same number will be started for each authority). If none
    /// is provided then the maximum number of workers will be started.
    /// If the `boot_wait_time` is provided then between node starts we'll wait for this
    /// time before the next node is started. This is useful to simulate staggered
    /// node starts. If none is provided then the nodes will be started immediately
    /// the one after the other.
    pub async fn start(
        &mut self,
        authorities_number: Option<usize>,
        workers_per_authority: Option<usize>,
        boot_wait_time: Option<Duration>,
    ) {
        let max_authorities = self.committee_shared.load().authorities.len();
        let authorities = authorities_number.unwrap_or(max_authorities);

        if authorities > max_authorities {
            panic!("Provided nodes number is greater than the maximum allowed");
        }

        for id in 0..authorities {
            info!("Spinning up node: {id}");
            self.start_node(id, false, workers_per_authority).await;

            if let Some(d) = boot_wait_time {
                // we don't want to wait after the last node has been boostraped
                if id < authorities - 1 {
                    info!(
                        "#### Will wait for {} seconds before starting the next node ####",
                        d.as_secs()
                    );
                    tokio::time::sleep(d).await;
                }
            }
        }
    }

    /// Starts the authority node by the defined id - if not already running - and
    /// the details are returned. If the node is already running then a panic
    /// is thrown instead.
    /// When the preserve_store is true, then the started authority will use the
    /// same path that has been used the last time when started (both the primary
    /// and the workers).
    /// This is basically a way to use the same storage between node restarts.
    /// When the preserve_store is false, then authority will start with an empty
    /// storage.
    /// If the `workers_per_authority` is provided then the corresponding number of
    /// workers will be started per authority. Otherwise if not provided, then maximum
    /// number of workers will be started per authority.
    pub async fn start_node(
        &mut self,
        id: usize,
        preserve_store: bool,
        workers_per_authority: Option<usize>,
    ) {
        let authority = self
            .authorities
            .get_mut(&id)
            .unwrap_or_else(|| panic!("Authority with id {} not found", id));

        // start the primary
        authority.start_primary(preserve_store).await;

        // start the workers
        if let Some(workers) = workers_per_authority {
            for worker_id in 0..workers {
                authority
                    .start_worker(worker_id as WorkerId, preserve_store)
                    .await;
            }
        } else {
            authority.start_all_workers(preserve_store).await;
        }
    }

    /// This method stops the authority (both the primary and the worker nodes)
    /// with the provided id.
    pub async fn stop_node(&self, id: usize) {
        if let Some(node) = self.authorities.get(&id) {
            node.stop_all().await;
            info!("Aborted node for id {id}");
        } else {
            info!("Node with {id} not found - nothing to stop");
        }
        // TODO: wait for the node's network port to be released.
    }

    /// Returns all the running authorities. Any authority that:
    /// * has been started ever
    /// * or has been stopped
    /// will not be returned by this method.
    pub async fn authorities(&self) -> Vec<AuthorityDetails> {
        let mut result = Vec::new();

        for authority in self.authorities.values() {
            if authority.is_running().await {
                result.push(authority.clone());
            }
        }

        result
    }

    /// Returns the authority identified by the provided id. Will panic if the
    /// authority with the id is not found. The returned authority can be freely
    /// cloned and managed without having the need to fetch again.
    pub fn authority(&self, id: usize) -> AuthorityDetails {
        self.authorities
            .get(&id)
            .unwrap_or_else(|| panic!("Authority with id {} not found", id))
            .clone()
    }

    /// This method asserts the progress of the cluster.
    /// `expected_nodes`: Nodes expected to have made progress. Any number different than that
    /// will make the assertion fail.
    /// `commit_threshold`: The acceptable threshold between the minimum and maximum reported
    /// commit value from the nodes.
    pub async fn assert_progress(
        &self,
        expected_nodes: u64,
        commit_threshold: u64,
    ) -> HashMap<usize, u64> {
        let r = self.authorities_latest_commit_round().await;
        let rounds: HashMap<usize, u64> = r
            .into_iter()
            .map(|(key, value)| (key, value as u64))
            .collect();

        assert_eq!(
            rounds.len(),
            expected_nodes as usize,
            "Expected to have received commit metrics from {expected_nodes} nodes"
        );
        assert!(rounds.values().all(|v| v > &1), "All nodes are available so all should have made progress and committed at least after the first round");

        if expected_nodes == 0 {
            return HashMap::new();
        }

        let (min, max) = rounds.values().into_iter().minmax().into_option().unwrap();
        assert!(
            max - min <= commit_threshold,
            "Nodes shouldn't be that behind"
        );

        rounds
    }

    async fn authorities_latest_commit_round(&self) -> HashMap<usize, f64> {
        let mut authorities_latest_commit = HashMap::new();

        for authority in self.authorities().await {
            let primary = authority.primary().await;
            if let Some(metric) = primary.metric("narwhal_primary_last_committed_round") {
                let value = metric.get_gauge().get_value();

                authorities_latest_commit.insert(primary.id, value);

                info!(
                    "[Node {}] Metric narwhal_primary_last_committed_round -> {value}",
                    primary.id
                );
            }
        }

        authorities_latest_commit
    }

    fn parameters() -> Parameters {
        Parameters {
            batch_size: 200,
            max_header_delay: Duration::from_secs(2),
            ..Parameters::default()
        }
    }
}

#[derive(Clone)]
pub struct PrimaryNodeDetails {
    pub id: usize,
    pub key_pair: Arc<KeyPair>,
    pub network_key_pair: Arc<NetworkKeyPair>,
    pub tx_transaction_confirmation: Sender<SerializedTransaction>,
    registry: Registry,
    store_path: PathBuf,
    committee: SharedCommittee,
    worker_cache: SharedWorkerCache,
    parameters: Parameters,
    handlers: Rc<RefCell<Vec<JoinHandle<()>>>>,
    internal_consensus_enabled: bool,
}

impl PrimaryNodeDetails {
    fn new(
        id: usize,
        key_pair: KeyPair,
        network_key_pair: NetworkKeyPair,
        parameters: Parameters,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        internal_consensus_enabled: bool,
    ) -> Self {
        // used just to initialise the struct value
        let (tx, _) = tokio::sync::broadcast::channel(1);

        Self {
            id,
            key_pair: Arc::new(key_pair),
            network_key_pair: Arc::new(network_key_pair),
            registry: Registry::new(),
            store_path: temp_dir(),
            tx_transaction_confirmation: tx,
            committee,
            worker_cache,
            parameters,
            handlers: Rc::new(RefCell::new(Vec::new())),
            internal_consensus_enabled,
        }
    }

    /// Returns the metric - if exists - identified by the provided name.
    /// If metric has not been found then None is returned instead.
    pub fn metric(&self, name: &str) -> Option<Metric> {
        let metrics = self.registry.gather();

        let metric = metrics.into_iter().find(|m| m.get_name() == name);
        metric.map(|m| m.get_metric().first().unwrap().clone())
    }

    async fn start(&mut self, preserve_store: bool) {
        if self.is_running() {
            panic!("Tried to start a node that is already running");
        }

        let registry = primary_metrics_registry(self.key_pair.public().clone());

        // Make the data store.
        let store_path = if preserve_store {
            self.store_path.clone()
        } else {
            temp_dir()
        };

        info!(
            "Primary Node {} will use path {:?}",
            self.id,
            store_path.clone()
        );

        // The channel returning the result for each transaction's execution.
        let (tx_transaction_confirmation, mut rx_transaction_confirmation) =
            channel(Node::CHANNEL_CAPACITY);

        // Primary node
        let primary_store: NodeStorage = NodeStorage::reopen(store_path.clone());
        let mut primary_handlers = Node::spawn_primary(
            self.key_pair.copy(),
            self.network_key_pair.copy(),
            self.committee.clone(),
            self.worker_cache.clone(),
            &primary_store,
            self.parameters.clone(),
            /* consensus */ self.internal_consensus_enabled,
            /* execution_state */
            Arc::new(SimpleExecutionState::new(tx_transaction_confirmation)),
            &registry,
        )
        .await
        .unwrap();

        let (tx, _) = tokio::sync::broadcast::channel(primary::CHANNEL_CAPACITY);
        let transactions_sender = tx.clone();
        // spawn a task to listen on the committed transactions
        // and translate to a mpmc channel
        let h = tokio::spawn(async move {
            while let Some(t) = rx_transaction_confirmation.recv().await {
                // send the transaction to the mpmc channel
                let _ = transactions_sender.send(t);
            }
        });

        // add the tasks's handle to the primary's handle so can be shutdown
        // with the others.
        primary_handlers.push(h);

        self.handlers.replace(primary_handlers);
        self.store_path = store_path;
        self.registry = registry;
        self.tx_transaction_confirmation = tx;
    }

    fn stop(&self) {
        self.handlers.borrow().iter().for_each(|h| h.abort());
        info!("Aborted primary node for id {}", self.id);
    }

    /// This method returns whether the node is still running or not. We
    /// iterate over all the handlers and check whether there is still any
    /// that is not finished. If we find at least one, then we report the
    /// node as still running.
    pub fn is_running(&self) -> bool {
        if self.handlers.borrow().is_empty() {
            return false;
        }

        self.handlers.borrow().iter().any(|h| !h.is_finished())
    }
}

#[derive(Clone)]
pub struct WorkerNodeDetails {
    pub id: WorkerId,
    pub transactions_address: Multiaddr,
    pub registry: Registry,
    name: PublicKey,
    committee: SharedCommittee,
    worker_cache: SharedWorkerCache,
    parameters: Parameters,
    store_path: PathBuf,
    handlers: Arc<ArcSwap<Vec<JoinHandle<()>>>>,
}

impl WorkerNodeDetails {
    fn new(
        id: WorkerId,
        name: PublicKey,
        parameters: Parameters,
        transactions_address: Multiaddr,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
    ) -> Self {
        Self {
            id,
            name,
            registry: Registry::new(),
            store_path: temp_dir(),
            transactions_address,
            committee,
            worker_cache,
            parameters,
            handlers: Arc::new(ArcSwap::from_pointee(Vec::new())),
        }
    }

    /// Starts the node. When preserve_store is true then the last used
    async fn start(&mut self, keypair: NetworkKeyPair, preserve_store: bool) {
        if self.is_running() {
            panic!(
                "Worker with id {} is already running, can't start again",
                self.id
            );
        }

        let registry = worker_metrics_registry(self.id, self.name.clone());

        // Make the data store.
        let store_path = if preserve_store {
            self.store_path.clone()
        } else {
            temp_dir()
        };

        let worker_store = NodeStorage::reopen(store_path.clone());
        let worker_handlers = Node::spawn_workers(
            self.name.clone(),
            vec![(self.id, keypair)],
            self.committee.clone(),
            self.worker_cache.clone(),
            &worker_store,
            self.parameters.clone(),
            TrivialTransactionValidator::default(),
            &registry,
        );

        self.handlers.swap(Arc::new(worker_handlers));
        self.store_path = store_path;
        self.registry = registry;
    }

    fn stop(&self) {
        self.handlers.load().iter().for_each(|h| h.abort());
        info!("Aborted worker node for id {}", self.id);
    }

    /// This method returns whether the node is still running or not. We
    /// iterate over all the handlers and check whether there is still any
    /// that is not finished. If we find at least one, then we report the
    /// node as still running.
    pub fn is_running(&self) -> bool {
        self.handlers.load().iter().any(|h| !h.is_finished())
    }
}

/// The authority details hold all the necessary structs and details
/// to identify and manage a specific authority. An authority is
/// composed of its primary node and the worker nodes. Via this struct
/// we can manage the nodes one by one or in batch fashion (ex stop_all).
/// The Authority can be cloned and reused across the instances as its
/// internals are thread safe. So changes made from one instance will be
/// reflected to another.
#[allow(dead_code)]
#[derive(Clone)]
pub struct AuthorityDetails {
    pub id: usize,
    pub name: PublicKey,
    internal: Arc<RwLock<AuthorityDetailsInternal>>,
}

struct AuthorityDetailsInternal {
    primary: PrimaryNodeDetails,
    worker_keypairs: Vec<NetworkKeyPair>,
    workers: HashMap<WorkerId, WorkerNodeDetails>,
}

impl AuthorityDetails {
    pub fn new(
        id: usize,
        key_pair: KeyPair,
        network_key_pair: NetworkKeyPair,
        worker_keypairs: Vec<NetworkKeyPair>,
        parameters: Parameters,
        committee: SharedCommittee,
        worker_cache: SharedWorkerCache,
        internal_consensus_enabled: bool,
    ) -> Self {
        // Create all the nodes we have in the committee
        let name = key_pair.public().clone();
        let primary = PrimaryNodeDetails::new(
            id,
            key_pair,
            network_key_pair,
            parameters.clone(),
            committee.clone(),
            worker_cache.clone(),
            internal_consensus_enabled,
        );

        // Create all the workers - even if we don't intend to start them all. Those
        // act as place holder setups. That gives us the power in a clear way manage
        // the nodes independently.
        let mut workers = HashMap::new();
        for (worker_id, addresses) in worker_cache.load().workers.get(&name).unwrap().0.clone() {
            let worker = WorkerNodeDetails::new(
                worker_id,
                name.clone(),
                parameters.clone(),
                addresses.transactions.clone(),
                committee.clone(),
                worker_cache.clone(),
            );
            workers.insert(worker_id, worker);
        }

        let internal = AuthorityDetailsInternal {
            primary,
            worker_keypairs,
            workers,
        };

        Self {
            id,
            name,
            internal: Arc::new(RwLock::new(internal)),
        }
    }

    /// Starts the node's primary and workers. If the num_of_workers is provided
    /// then only those ones will be started. Otherwise all the available workers
    /// will be started instead.
    /// If the preserve_store value is true then the previous node's storage
    /// will be preserved. If false then the node will  start with a fresh
    /// (empty) storage.
    pub async fn start(&self, preserve_store: bool, num_of_workers: Option<usize>) {
        self.start_primary(preserve_store).await;

        let workers_to_start;
        {
            let internal = self.internal.read().await;
            workers_to_start = num_of_workers.unwrap_or(internal.workers.len());
        }

        for id in 0..workers_to_start {
            self.start_worker(id as WorkerId, preserve_store).await;
        }
    }

    /// Starts the primary node. If the preserve_store value is true then the
    /// previous node's storage will be preserved. If false then the node will
    /// start with a fresh (empty) storage.
    pub async fn start_primary(&self, preserve_store: bool) {
        let mut internal = self.internal.write().await;

        internal.primary.start(preserve_store).await;
    }

    pub async fn stop_primary(&self) {
        let internal = self.internal.read().await;

        internal.primary.stop();
    }

    pub async fn start_all_workers(&self, preserve_store: bool) {
        let mut internal = self.internal.write().await;
        let worker_keypairs = internal
            .worker_keypairs
            .iter()
            .map(|kp| kp.copy())
            .collect::<Vec<NetworkKeyPair>>();

        for (id, worker) in internal.workers.iter_mut() {
            let keypair = worker_keypairs.get(*id as usize).unwrap().copy();
            worker.start(keypair, preserve_store).await;
        }
    }

    /// Starts the worker node by the provided id. If worker is not found then
    /// a panic is raised. If the preserve_store value is true then the
    /// previous node's storage will be preserved. If false then the node will
    /// start with a fresh (empty) storage.
    pub async fn start_worker(&self, id: WorkerId, preserve_store: bool) {
        let mut internal = self.internal.write().await;
        let keypair = internal.worker_keypairs.get(id as usize).unwrap().copy();
        let worker = internal
            .workers
            .get_mut(&id)
            .unwrap_or_else(|| panic!("Worker with id {} not found ", id));

        worker.start(keypair, preserve_store).await;
    }

    pub async fn stop_worker(&self, id: WorkerId) {
        let internal = self.internal.read().await;

        internal
            .workers
            .get(&id)
            .unwrap_or_else(|| panic!("Worker with id {} not found ", id))
            .stop();
    }

    /// Stops all the nodes (primary & workers).
    pub async fn stop_all(&self) {
        let internal = self.internal.read().await;

        internal.primary.stop();

        for (_, worker) in internal.workers.iter() {
            worker.stop();
        }
    }

    /// Will restart the node with the current setup that has been chosen
    /// (ex same number of nodes).
    /// `preserve_store`: if true then the same storage will be used for the
    /// node
    /// `delay`: before starting again we'll wait for that long. If zero provided
    /// then won't wait at all
    pub async fn restart(&self, preserve_store: bool, delay: Duration) {
        let num_of_workers = self.workers().await.len();

        self.stop_all().await;

        tokio::time::sleep(delay).await;

        // now start again the node with the same workers
        self.start(preserve_store, Some(num_of_workers)).await;
    }

    /// Returns the current primary node running as a clone. If the primary
    ///node stops and starts again and it's needed by the user then this
    /// method should be called again to get the latest one.
    pub async fn primary(&self) -> PrimaryNodeDetails {
        let internal = self.internal.read().await;

        internal.primary.clone()
    }

    /// Returns the worker with the provided id. If not found then a panic
    /// is raised instead. If the worker is stopped and started again then
    /// the worker will need to be fetched again via this method.
    pub async fn worker(&self, id: WorkerId) -> WorkerNodeDetails {
        let internal = self.internal.read().await;

        internal
            .workers
            .get(&id)
            .unwrap_or_else(|| panic!("Worker with id {} not found ", id))
            .clone()
    }

    /// Helper method to return transaction addresses of
    /// all the worker nodes.
    /// Important: only the addresses of the running workers will
    /// be returned.
    pub async fn worker_transaction_addresses(&self) -> Vec<Multiaddr> {
        self.workers()
            .await
            .iter()
            .map(|w| w.transactions_address.clone())
            .collect()
    }

    /// Returns all the running workers
    async fn workers(&self) -> Vec<WorkerNodeDetails> {
        let internal = self.internal.read().await;

        internal
            .workers
            .iter()
            .filter_map(|(_, worker)| {
                if worker.is_running() {
                    Some(worker.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Creates a new proposer client that connects to the corresponding client.
    /// This should be available only if the internal consensus is disabled. If
    /// the internal consensus is enabled then a panic will be thrown instead.
    pub async fn new_proposer_client(&self) -> ProposerClient<Channel> {
        let internal = self.internal.read().await;

        if internal.primary.internal_consensus_enabled {
            panic!("External consensus is disabled, won't create a proposer client");
        }

        let config = mysten_network::config::Config::new();
        let channel = config
            .connect_lazy(&internal.primary.parameters.consensus_api_grpc.socket_addr)
            .unwrap();

        ProposerClient::new(channel)
    }

    /// This method returns a new client to send transactions to the dictated
    /// worker identified by the `worker_id`. If the worker_id is not found then
    /// a panic is raised.
    pub async fn new_transactions_client(
        &self,
        worker_id: &WorkerId,
    ) -> TransactionsClient<Channel> {
        let internal = self.internal.read().await;

        let config = mysten_network::config::Config::new();
        let channel = config
            .connect_lazy(
                &internal
                    .workers
                    .get(worker_id)
                    .unwrap()
                    .transactions_address,
            )
            .unwrap();

        TransactionsClient::new(channel)
    }

    /// Creates a new configuration client that connects to the corresponding client.
    /// This should be available only if the internal consensus is disabled. If
    /// the internal consensus is enabled then a panic will be thrown instead.
    pub async fn new_configuration_client(&self) -> ConfigurationClient<Channel> {
        let internal = self.internal.read().await;

        if internal.primary.internal_consensus_enabled {
            panic!("External consensus is disabled, won't create a configuration client");
        }

        let config = mysten_network::config::Config::new();
        let channel = config
            .connect_lazy(&internal.primary.parameters.consensus_api_grpc.socket_addr)
            .unwrap();

        ConfigurationClient::new(channel)
    }

    /// This method will return true either when the primary or any of
    /// the workers is running. In order to make sure that we don't end up
    /// in intermediate states we want to make sure that everything has
    /// stopped before we report something as not running (in case we want
    /// to start them again).
    async fn is_running(&self) -> bool {
        let internal = self.internal.read().await;

        if internal.primary.is_running() {
            return true;
        }

        for (_, worker) in internal.workers.iter() {
            if worker.is_running() {
                return true;
            }
        }
        false
    }
}

pub fn setup_tracing() -> TelemetryGuards {
    // Setup tracing
    let tracing_level = "debug";
    let network_tracing_level = "info";

    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level},quinn={network_tracing_level}");

    telemetry_subscribers::TelemetryConfig::new("narwhal")
        // load env variables
        .with_env()
        // load special log filter
        .with_log_level(&log_filter)
        .init()
        .0
}
