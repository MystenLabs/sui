// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::new_registry;
use crate::{try_join_all, FuturesUnordered, NodeError};
use anemo::PeerId;
use arc_swap::{ArcSwap, ArcSwapOption};
use config::{Committee, Parameters, WorkerCache, WorkerId};
use crypto::{NetworkKeyPair, PublicKey};
use fastcrypto::traits::KeyPair;
use mysten_metrics::{RegistryID, RegistryService};
use network::client::NetworkClient;
use prometheus::Registry;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use storage::NodeStorage;
use sui_protocol_config::ProtocolConfig;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, instrument};
use types::PreSubscribedBroadcastSender;
use worker::metrics::{initialise_metrics, Metrics};
use worker::{TransactionValidator, Worker, NUM_SHUTDOWN_RECEIVERS};

pub struct WorkerNodeInner {
    // The worker's id
    id: WorkerId,
    protocol_config: ProtocolConfig,
    // The configuration parameters.
    parameters: Parameters,
    // A prometheus RegistryService to use for the metrics
    registry_service: RegistryService,
    // The latest registry id & registry used for the node
    registry: Option<(RegistryID, Registry)>,
    // The task handles created from primary
    handles: FuturesUnordered<JoinHandle<()>>,
    // The shutdown signal channel
    tx_shutdown: Option<PreSubscribedBroadcastSender>,
    // Peer ID used for local connections.
    own_peer_id: Option<PeerId>,
}

impl WorkerNodeInner {
    // Starts the worker node with the provided info. If the node is already running then this
    // method will return an error instead.
    #[instrument(level = "info", skip_all)]
    async fn start(
        &mut self,
        // The primary's id
        primary_name: PublicKey,
        // The private-public network key pair of this authority.
        network_keypair: NetworkKeyPair,
        // The committee information.
        committee: Committee,
        // The worker information cache.
        worker_cache: WorkerCache,
        // Client for communications.
        client: NetworkClient,
        // The node's store
        // TODO: replace this by a path so the method can open and independent storage
        store: &NodeStorage,
        // The transaction validator that should be used
        tx_validator: impl TransactionValidator,
        // Optionally, if passed, then this metrics struct should be used instead of creating our
        // own one.
        metrics: Option<Metrics>,
    ) -> Result<(), NodeError> {
        if self.is_running().await {
            return Err(NodeError::NodeAlreadyRunning);
        }

        self.own_peer_id = Some(PeerId(network_keypair.public().0.to_bytes()));

        let (metrics, registry) = if let Some(metrics) = metrics {
            (metrics, None)
        } else {
            // create a new registry
            let registry = new_registry();

            (initialise_metrics(&registry), Some(registry))
        };

        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

        let authority = committee
            .authority_by_key(&primary_name)
            .unwrap_or_else(|| {
                panic!(
                    "Our node with key {:?} should be in committee",
                    primary_name
                )
            });

        let handles = Worker::spawn(
            authority.clone(),
            network_keypair,
            self.id,
            committee.clone(),
            worker_cache.clone(),
            self.protocol_config.clone(),
            self.parameters.clone(),
            tx_validator.clone(),
            client.clone(),
            store.batch_store.clone(),
            metrics,
            &mut tx_shutdown,
        );

        // store the registry
        if let Some(registry) = registry {
            self.swap_registry(Some(registry));
        }

        // now keep the handlers
        self.handles.clear();
        self.handles.extend(handles);
        self.tx_shutdown = Some(tx_shutdown);

        Ok(())
    }

    // Will shutdown the worker node and wait until the node has shutdown by waiting on the
    // underlying components handles. If the node was not already running then the
    // method will return immediately.
    #[instrument(level = "info", skip_all)]
    async fn shutdown(&mut self) {
        if !self.is_running().await {
            return;
        }

        let now = Instant::now();
        if let Some(tx_shutdown) = self.tx_shutdown.as_ref() {
            tx_shutdown
                .send()
                .expect("Couldn't send the shutdown signal to downstream components");
            self.tx_shutdown = None;
        }

        // Now wait until handles have been completed
        try_join_all(&mut self.handles).await.unwrap();

        self.swap_registry(None);

        info!(
            "Narwhal worker {} shutdown is complete - took {} seconds",
            self.id,
            now.elapsed().as_secs_f64()
        );
    }

    // If any of the underlying handles haven't still finished, then this method will return
    // true, otherwise false will returned instead.
    async fn is_running(&self) -> bool {
        self.handles.iter().any(|h| !h.is_finished())
    }

    // Helper method useful to wait on the execution of the primary node
    async fn wait(&mut self) {
        try_join_all(&mut self.handles).await.unwrap();
    }

    // Accepts an Option registry. If it's Some, then the new registry will be added in the
    // registry service and the registry_id will be updated. Also, any previous registry will
    // be removed. If None is passed, then the registry_id is updated to None and any old
    // registry is removed from the RegistryService.
    fn swap_registry(&mut self, registry: Option<Registry>) {
        if let Some((registry_id, _registry)) = self.registry.as_ref() {
            self.registry_service.remove(*registry_id);
        }

        if let Some(registry) = registry {
            self.registry = Some((self.registry_service.add(registry.clone()), registry));
        } else {
            self.registry = None
        }
    }
}

#[derive(Clone)]
pub struct WorkerNode {
    internal: Arc<RwLock<WorkerNodeInner>>,
}

impl WorkerNode {
    pub fn new(
        id: WorkerId,
        protocol_config: ProtocolConfig,
        parameters: Parameters,
        registry_service: RegistryService,
    ) -> WorkerNode {
        let inner = WorkerNodeInner {
            id,
            protocol_config,
            parameters,
            registry_service,
            registry: None,
            handles: FuturesUnordered::new(),
            tx_shutdown: None,
            own_peer_id: None,
        };

        Self {
            internal: Arc::new(RwLock::new(inner)),
        }
    }

    pub async fn start(
        &self,
        // The primary's public key of this authority.
        primary_key: PublicKey,
        // The private-public network key pair of this authority.
        network_keypair: NetworkKeyPair,
        // The committee information.
        committee: Committee,
        // The worker information cache.
        worker_cache: WorkerCache,
        // Client for communications.
        client: NetworkClient,
        // The node's store
        // TODO: replace this by a path so the method can open and independent storage
        store: &NodeStorage,
        // The transaction validator defining Tx acceptance,
        tx_validator: impl TransactionValidator,
        // An optional metrics struct
        metrics: Option<Metrics>,
    ) -> Result<(), NodeError> {
        let mut guard = self.internal.write().await;
        guard
            .start(
                primary_key,
                network_keypair,
                committee,
                worker_cache,
                client,
                store,
                tx_validator,
                metrics,
            )
            .await
    }

    pub async fn shutdown(&self) {
        let mut guard = self.internal.write().await;
        guard.shutdown().await
    }

    pub async fn is_running(&self) -> bool {
        let guard = self.internal.read().await;
        guard.is_running().await
    }

    pub async fn wait(&self) {
        let mut guard = self.internal.write().await;
        guard.wait().await
    }
}

pub struct WorkerNodes {
    workers: ArcSwap<HashMap<WorkerId, WorkerNode>>,
    registry_service: RegistryService,
    registry_id: ArcSwapOption<RegistryID>,
    parameters: Parameters,
    client: ArcSwapOption<NetworkClient>,
}

impl WorkerNodes {
    pub fn new(registry_service: RegistryService, parameters: Parameters) -> Self {
        Self {
            workers: ArcSwap::from(Arc::new(HashMap::default())),
            registry_service,
            registry_id: ArcSwapOption::empty(),
            parameters,
            client: ArcSwapOption::empty(),
        }
    }

    #[instrument(level = "info", skip_all)]
    pub async fn start(
        &self,
        // The primary's public key of this authority.
        primary_key: PublicKey,
        // The ids & keypairs of the workers to spawn.
        ids_and_keypairs: Vec<(WorkerId, NetworkKeyPair)>,
        // The committee information.
        committee: Committee,
        protocol_config: ProtocolConfig,
        // The worker information cache.
        worker_cache: WorkerCache,
        // Client for communications.
        client: NetworkClient,
        // The node's store
        // TODO: replace this by a path so the method can open and independent storage
        store: &NodeStorage,
        // The transaction validator defining Tx acceptance,
        tx_validator: impl TransactionValidator,
    ) -> Result<(), NodeError> {
        let worker_ids_running = self.workers_running().await;
        if !worker_ids_running.is_empty() {
            return Err(NodeError::WorkerNodesAlreadyRunning(worker_ids_running));
        }

        // create the registry first
        let registry = new_registry();

        let metrics = initialise_metrics(&registry);

        self.client.store(Some(Arc::new(client.clone())));

        // now clear the previous handles - we want to do that proactively
        // as it's not guaranteed that shutdown has been called
        self.workers.store(Arc::new(HashMap::default()));

        let mut workers = HashMap::<WorkerId, WorkerNode>::new();
        // start all the workers one by one
        for (worker_id, key_pair) in ids_and_keypairs {
            let worker = WorkerNode::new(
                worker_id,
                protocol_config.clone(),
                self.parameters.clone(),
                self.registry_service.clone(),
            );

            worker
                .start(
                    primary_key.clone(),
                    key_pair,
                    committee.clone(),
                    worker_cache.clone(),
                    client.clone(),
                    store,
                    tx_validator.clone(),
                    Some(metrics.clone()),
                )
                .await?;

            workers.insert(worker_id, worker);
        }

        // update the worker handles.
        self.workers.store(Arc::new(workers));

        // now add the registry
        let registry_id = self.registry_service.add(registry);

        if let Some(old_registry_id) = self.registry_id.swap(Some(Arc::new(registry_id))) {
            // a little of defensive programming - ensure that we always clean up the previous registry
            self.registry_service.remove(*old_registry_id.as_ref());
        }

        Ok(())
    }

    // Shuts down all the workers
    #[instrument(level = "info", skip_all)]
    pub async fn shutdown(&self) {
        if let Some(client) = self.client.load_full() {
            client.shutdown();
        }

        for (key, worker) in self.workers.load_full().as_ref() {
            info!("Shutting down worker {}", key);
            worker.shutdown().await;
        }

        // now remove the registry id
        if let Some(old_registry_id) = self.registry_id.swap(None) {
            // a little of defensive programming - ensure that we always clean up the previous registry
            self.registry_service.remove(*old_registry_id.as_ref());
        }

        // now clean up the worker handles
        self.workers.store(Arc::new(HashMap::default()));
    }

    // returns the worker ids that are currently running
    pub async fn workers_running(&self) -> Vec<WorkerId> {
        let mut worker_ids = Vec::new();

        for (id, worker) in self.workers.load_full().as_ref() {
            if worker.is_running().await {
                worker_ids.push(*id);
            }
        }

        worker_ids
    }
}
