// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{try_join_all, FuturesUnordered, NodeError};
use arc_swap::ArcSwap;
use config::{Parameters, SharedCommittee, SharedWorkerCache, WorkerId};
use crypto::{NetworkKeyPair, PublicKey};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use storage::NodeStorage;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{info, instrument};
use types::PreSubscribedBroadcastSender;
use worker::{TransactionValidator, Worker, NUM_SHUTDOWN_RECEIVERS};

pub struct WorkerNodeInner {
    // The worker's id
    id: WorkerId,
    // The configuration parameters.
    parameters: Parameters,
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
    ) -> Result<(), NodeError> {
        if self.is_running().await {
            return Err(NodeError::NodeAlreadyRunning);
        }

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
            self.parameters.clone(),
            tx_validator.clone(),
            client.clone(),
            store.batch_store.clone(),
            &mut tx_shutdown,
        );

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
}

#[derive(Clone)]
pub struct WorkerNode {
    internal: Arc<RwLock<WorkerNodeInner>>,
}

impl WorkerNode {
    pub fn new(id: WorkerId, parameters: Parameters) -> WorkerNode {
        let inner = WorkerNodeInner {
            id,
            parameters,
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
    parameters: Parameters,
    client: ArcSwapOption<NetworkClient>,
}

impl WorkerNodes {
    pub fn new(parameters: Parameters) -> Self {
        Self {
            workers: ArcSwap::from(Arc::new(HashMap::default())),
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

        // now clear the previous handles - we want to do that proactively
        // as it's not guaranteed that shutdown has been called
        self.workers.store(Arc::new(HashMap::default()));

        let mut workers = HashMap::<WorkerId, WorkerNode>::new();
        // start all the workers one by one
        for (worker_id, key_pair) in ids_and_keypairs {
            let worker = WorkerNode::new(worker_id, self.parameters.clone());

            worker
                .start(
                    primary_key.clone(),
                    key_pair,
                    committee.clone(),
                    worker_cache.clone(),
                    client.clone(),
                    store,
                    tx_validator.clone(),
                )
                .await?;

            workers.insert(worker_id, worker);
        }

        // update the worker handles.
        self.workers.store(Arc::new(workers));

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
