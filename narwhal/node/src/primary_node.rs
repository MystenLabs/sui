// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{try_join_all, FuturesUnordered, NodeError};
use anemo::PeerId;
use config::{AuthorityIdentifier, Committee, Parameters, WorkerCache};
use consensus::bullshark::Bullshark;
use consensus::consensus::ConsensusRound;
use consensus::dag::Dag;
use consensus::Consensus;
use crypto::{KeyPair, NetworkKeyPair, PublicKey};
use executor::{get_restored_consensus_output, ExecutionState, Executor, SubscriberResult};
use fastcrypto::traits::{KeyPair as _, VerifyingKey};
use primary::{NetworkModel, Primary, NUM_SHUTDOWN_RECEIVERS};
use std::sync::Arc;
use std::time::Instant;
use storage::NodeStorage;
use tokio::sync::{mpsc, oneshot, watch, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument};
use types::{Certificate, ConditionalBroadcastReceiver, PreSubscribedBroadcastSender, Round};

struct PrimaryNodeInner {
    // The configuration parameters.
    parameters: Parameters,
    // Whether to run consensus (and an executor client) or not.
    // If true, an internal consensus will be used, else an external consensus will be used.
    // If an external consensus will be used, then this bool will also ensure that the
    // corresponding gRPC server that is used for communication between narwhal and
    // external consensus is also spawned.
    internal_consensus: bool,
    // The task handles created from primary
    handles: FuturesUnordered<JoinHandle<()>>,
    // Keeping NetworkClient here for quicker shutdown.
    client: Option<NetworkClient>,
    // The shutdown signal channel
    tx_shutdown: Option<PreSubscribedBroadcastSender>,
    // Peer ID used for local connections.
    own_peer_id: Option<PeerId>,
}

impl PrimaryNodeInner {
    /// The default channel capacity.
    pub const CHANNEL_CAPACITY: usize = 1_000;
    /// The window where the schedule change takes place in consensus. It represents number
    /// of committed sub dags.
    /// TODO: move this to node properties
    const CONSENSUS_SCHEDULE_CHANGE_SUB_DAGS: u64 = 300;

    // Starts the primary node with the provided info. If the node is already running then this
    // method will return an error instead.
    #[instrument(level = "info", skip_all)]
    async fn start<State>(
        &mut self, // The private-public key pair of this authority.
        keypair: KeyPair,
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
        // The state used by the client to execute transactions.
        execution_state: Arc<State>,
    ) -> Result<(), NodeError>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        if self.is_running().await {
            return Err(NodeError::NodeAlreadyRunning);
        }

        // create the channel to send the shutdown signal
        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

        // spawn primary if not already running
        let handles = Self::spawn_primary(
            keypair,
            network_keypair,
            committee,
            worker_cache,
            client,
            store,
            self.parameters.clone(),
            self.internal_consensus,
            execution_state,
            &mut tx_shutdown,
        )
        .await?;

        // now keep the handlers
        self.handles.clear();
        self.handles.extend(handles);
        self.tx_shutdown = Some(tx_shutdown);

        Ok(())
    }

    // Will shutdown the primary node and wait until the node has shutdown by waiting on the
    // underlying components handles. If the node was not already running then the
    // method will return immediately.
    #[instrument(level = "info", skip_all)]
    async fn shutdown(&mut self) {
        if !self.is_running().await {
            return;
        }

        // send the shutdown signal to the node
        let now = Instant::now();
        info!("Sending shutdown message to primary node");

        if let Some(c) = self.client.take() {
            c.shutdown();
        }

        if let Some(tx_shutdown) = self.tx_shutdown.as_ref() {
            tx_shutdown
                .send()
                .expect("Couldn't send the shutdown signal to downstream components");
            self.tx_shutdown = None
        }

        // Now wait until handles have been completed
        try_join_all(&mut self.handles).await.unwrap();

        info!(
            "Narwhal primary shutdown is complete - took {} seconds",
            now.elapsed().as_secs_f64()
        );
    }

    // Helper method useful to wait on the execution of the primary node
    async fn wait(&mut self) {
        try_join_all(&mut self.handles).await.unwrap();
    }

    // If any of the underlying handles haven't still finished, then this method will return
    // true, otherwise false will returned instead.
    async fn is_running(&self) -> bool {
        self.handles.iter().any(|h| !h.is_finished())
    }

    /// Spawn a new primary. Optionally also spawn the consensus and a client executing transactions.
    pub async fn spawn_primary<State>(
        // The private-public key pair of this authority.
        keypair: KeyPair,
        // The private-public network key pair of this authority.
        network_keypair: NetworkKeyPair,
        // The committee information.
        committee: Committee,
        // The worker information cache.
        worker_cache: WorkerCache,
        // Client for communications.
        client: NetworkClient,
        // The node's storage.
        store: &NodeStorage,
        // The configuration parameters.
        parameters: Parameters,
        // Whether to run consensus (and an executor client) or not.
        // If true, an internal consensus will be used, else an external consensus will be used.
        // If an external consensus will be used, then this bool will also ensure that the
        // corresponding gRPC server that is used for communication between narwhal and
        // external consensus is also spawned.
        internal_consensus: bool,
        // The state used by the client to execute transactions.
        execution_state: Arc<State>,
        // The channel to send the shutdown signal
        tx_shutdown: &mut PreSubscribedBroadcastSender,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        let (tx_new_certificates, rx_new_certificates) = mpsc::channel(Self::CHANNEL_CAPACITY);

        let (tx_committed_certificates, rx_committed_certificates) =
            mpsc::channel(Self::CHANNEL_CAPACITY);

        // Compute the public key of this authority.
        let name = keypair.public().clone();

        // Figure out the id for this authority
        let authority = committee
            .authority_by_key(&name)
            .unwrap_or_else(|| panic!("Our node with key {:?} should be in committee", name));

        let mut handles = Vec::new();
        let (tx_consensus_round_updates, rx_consensus_round_updates) =
            watch::channel(ConsensusRound::new(0, 0));
        let (dag, network_model) = if !internal_consensus {
            debug!("Consensus is disabled: the primary will run w/o Bullshark");
            let (handle, dag) = Dag::new(
                &committee,
                rx_new_certificates,
                tx_shutdown.subscribe(),
            );

            handles.push(handle);

            (Some(Arc::new(dag)), NetworkModel::Asynchronous)
        } else {
            let consensus_handles = Self::spawn_consensus(
                authority.id(),
                worker_cache.clone(),
                committee.clone(),
                client.clone(),
                store,
                parameters.clone(),
                execution_state,
                tx_shutdown.subscribe_n(3),
                rx_new_certificates,
                tx_committed_certificates.clone(),
                tx_consensus_round_updates,
            )
            .await?;

            handles.extend(consensus_handles);

            (None, NetworkModel::PartiallySynchronous)
        };

        // TODO: the same set of variables are sent to primary, consensus and downstream
        // components. Consider using a holder struct to pass them around.

        // Spawn the primary.
        let primary_handles = Primary::spawn(
            authority.clone(),
            keypair,
            network_keypair,
            committee.clone(),
            worker_cache.clone(),
            parameters.clone(),
            client,
            store.header_store.clone(),
            store.certificate_store.clone(),
            store.proposer_store.clone(),
            store.payload_store.clone(),
            store.vote_digest_store.clone(),
            tx_new_certificates,
            rx_committed_certificates,
            rx_consensus_round_updates,
            dag,
            network_model,
            tx_shutdown,
            tx_committed_certificates,
            Some(tx_executor_network),
        );
        handles.extend(primary_handles);

        Ok(handles)
    }

    /// Spawn the consensus core and the client executing transactions.
    async fn spawn_consensus<State>(
        authority_id: AuthorityIdentifier,
        worker_cache: WorkerCache,
        committee: Committee,
        client: NetworkClient,
        store: &NodeStorage,
        parameters: Parameters,
        execution_state: State,
        mut shutdown_receivers: Vec<ConditionalBroadcastReceiver>,
        rx_new_certificates: mpsc::Receiver<Certificate>,
        tx_committed_certificates: mpsc::Sender<(Round, Vec<Certificate>)>,
        tx_consensus_round_updates: watch::Sender<Round>,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        PublicKey: VerifyingKey,
        State: ExecutionState + Send + Sync + 'static,
    {
        let (tx_sequence, rx_sequence) = mpsc::channel(Self::CHANNEL_CAPACITY);

        // Check for any sub-dags that have been sent by consensus but were not processed by the executor.
        let restored_consensus_output = get_restored_consensus_output(
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            &execution_state,
        )
        .await?;

        let num_sub_dags = restored_consensus_output.len() as u64;
        if num_sub_dags > 0 {
            info!(
                "Consensus output on its way to the executor was restored for {num_sub_dags} sub-dags",
            );
        }

        // TODO(metrics): Increment recovered_consensus_output by `num_sub_dags`

        // Spawn the consensus core who only sequences transactions.
        let ordering_engine = Bullshark::new(
            committee.clone(),
            store.consensus_store.clone(),
            parameters.gc_depth,
        );
        let consensus_handles = Consensus::spawn(
            committee.clone(),
            parameters.gc_depth,
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            shutdown_receivers.pop().unwrap(),
            rx_new_certificates,
            tx_committed_certificates,
            tx_consensus_round_updates,
            tx_sequence,
            ordering_engine,
        );

        // Spawn the client executing the transactions. It can also synchronize with the
        // subscriber handler if it missed some transactions.
        let executor_handles = Executor::spawn(
            authority_id,
            worker_cache,
            committee.clone(),
            client,
            execution_state,
            shutdown_receivers,
            rx_sequence,
            restored_consensus_output,
        )?;

        Ok(executor_handles
            .into_iter()
            .chain(std::iter::once(consensus_handles))
            .collect())
    }
}

#[derive(Clone)]
pub struct PrimaryNode {
    internal: Arc<RwLock<PrimaryNodeInner>>,
}

impl PrimaryNode {
    pub fn new(parameters: Parameters, internal_consensus: bool) -> PrimaryNode {
        let inner = PrimaryNodeInner {
            parameters,
            internal_consensus,
            handles: FuturesUnordered::new(),
            client: None,
            tx_shutdown: None,
            own_peer_id: None,
        };

        Self {
            internal: Arc::new(RwLock::new(inner)),
        }
    }

    pub async fn start<State>(
        &self, // The private-public key pair of this authority.
        keypair: KeyPair,
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
        // The state used by the client to execute transactions.
        execution_state: Arc<State>,
    ) -> Result<(), NodeError>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        let mut guard = self.internal.write().await;
        guard.client = Some(client.clone());
        guard
            .start(
                keypair,
                network_keypair,
                committee,
                worker_cache,
                client,
                store,
                execution_state,
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
