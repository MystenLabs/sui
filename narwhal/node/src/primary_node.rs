// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::new_registry;
use crate::{try_join_all, FuturesUnordered, NodeError};
use config::{Parameters, SharedCommittee, SharedWorkerCache};
use consensus::bullshark::Bullshark;
use consensus::dag::Dag;
use consensus::metrics::{ChannelMetrics, ConsensusMetrics};
use consensus::Consensus;
use crypto::{KeyPair, NetworkKeyPair, PublicKey};
use executor::{get_restored_consensus_output, ExecutionState, Executor, SubscriberResult};
use fastcrypto::traits::{KeyPair as _, VerifyingKey};
use mysten_metrics::{RegistryID, RegistryService};
use primary::{NetworkModel, Primary, PrimaryChannelMetrics, NUM_SHUTDOWN_RECEIVERS};
use prometheus::{IntGauge, Registry};
use std::sync::Arc;
use std::time::Instant;
use storage::NodeStorage;
use tokio::sync::{oneshot, watch, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, instrument};
use types::{
    metered_channel, Certificate, ConditionalBroadcastReceiver, PreSubscribedBroadcastSender, Round,
};

struct PrimaryNodeInner {
    // The configuration parameters.
    parameters: Parameters,
    // Whether to run consensus (and an executor client) or not.
    // If true, an internal consensus will be used, else an external consensus will be used.
    // If an external consensus will be used, then this bool will also ensure that the
    // corresponding gRPC server that is used for communication between narwhal and
    // external consensus is also spawned.
    internal_consensus: bool,
    // A prometheus RegistryService to use for the metrics
    registry_service: RegistryService,
    // The latest registry id & registry used for the node
    registry: Option<(RegistryID, Registry)>,
    // The task handles created from primary
    handles: FuturesUnordered<JoinHandle<()>>,
    // The shutdown signal channel
    tx_shutdown: Option<PreSubscribedBroadcastSender>,
}

impl PrimaryNodeInner {
    /// The default channel capacity.
    pub const CHANNEL_CAPACITY: usize = 1_000;

    // Starts the primary node with the provided info. If the node is already running then this
    // method will return an error instead.
    #[instrument(level = "info", skip_all)]
    async fn start<State>(
        &mut self, // The private-public key pair of this authority.
        keypair: KeyPair,
        // The private-public network key pair of this authority.
        network_keypair: NetworkKeyPair,
        // The committee information.
        committee: SharedCommittee,
        // The worker information cache.
        worker_cache: SharedWorkerCache,
        // The node's store //TODO: replace this by a path so the method can open and independent storage
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

        // create a new registry
        let registry = new_registry();

        // create the channel to send the shutdown signal
        let mut tx_shutdown = PreSubscribedBroadcastSender::new(NUM_SHUTDOWN_RECEIVERS);

        // spawn primary if not already running
        let handles = Self::spawn_primary(
            keypair,
            network_keypair,
            committee,
            worker_cache,
            store,
            self.parameters.clone(),
            self.internal_consensus,
            execution_state,
            &registry,
            &mut tx_shutdown,
        )
        .await?;

        // store the registry
        self.swap_registry(Some(registry));

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

        if let Some(tx_shutdown) = self.tx_shutdown.as_ref() {
            tx_shutdown
                .send()
                .expect("Couldn't send the shutdown signal to downstream components");
            self.tx_shutdown = None
        }

        // Now wait until handles have been completed
        try_join_all(&mut self.handles).await.unwrap();

        self.swap_registry(None);

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

    /// Spawn a new primary. Optionally also spawn the consensus and a client executing transactions.
    pub async fn spawn_primary<State>(
        // The private-public key pair of this authority.
        keypair: KeyPair,
        // The private-public network key pair of this authority.
        network_keypair: NetworkKeyPair,
        // The committee information.
        committee: SharedCommittee,
        // The worker information cache.
        worker_cache: SharedWorkerCache,
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
        // A prometheus exporter Registry to use for the metrics
        registry: &Registry,
        // The channel to send the shutdown signal
        tx_shutdown: &mut PreSubscribedBroadcastSender,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        // These gauge is porcelain: do not modify it without also modifying `primary::metrics::PrimaryChannelMetrics::replace_registered_new_certificates_metric`
        // This hack avoids a cyclic dependency in the initialization of consensus and primary
        let new_certificates_counter = IntGauge::new(
            PrimaryChannelMetrics::NAME_NEW_CERTS,
            PrimaryChannelMetrics::DESC_NEW_CERTS,
        )
        .unwrap();
        let (tx_new_certificates, rx_new_certificates) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &new_certificates_counter);

        let committed_certificates_counter = IntGauge::new(
            PrimaryChannelMetrics::NAME_COMMITTED_CERTS,
            PrimaryChannelMetrics::DESC_COMMITTED_CERTS,
        )
        .unwrap();
        let (tx_committed_certificates, rx_committed_certificates) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &committed_certificates_counter);

        // Compute the public key of this authority.
        let name = keypair.public().clone();
        let mut handles = Vec::new();
        let (tx_executor_network, rx_executor_network) = oneshot::channel();
        let (tx_consensus_round_updates, rx_consensus_round_updates) = watch::channel(0u64);
        let (dag, network_model) = if !internal_consensus {
            debug!("Consensus is disabled: the primary will run w/o Bullshark");
            let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));
            let (handle, dag) = Dag::new(
                &committee.load(),
                rx_new_certificates,
                consensus_metrics,
                tx_shutdown.subscribe(),
            );

            handles.push(handle);

            (Some(Arc::new(dag)), NetworkModel::Asynchronous)
        } else {
            let consensus_handles = Self::spawn_consensus(
                name.clone(),
                rx_executor_network,
                worker_cache.clone(),
                committee.clone(),
                store,
                parameters.clone(),
                execution_state,
                tx_shutdown.subscribe_n(3),
                rx_new_certificates,
                tx_committed_certificates.clone(),
                tx_consensus_round_updates,
                registry,
            )
            .await?;

            handles.extend(consensus_handles);

            (None, NetworkModel::PartiallySynchronous)
        };

        // Spawn the primary.
        let primary_handles = Primary::spawn(
            name.clone(),
            keypair,
            network_keypair,
            committee.clone(),
            worker_cache.clone(),
            parameters.clone(),
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
            registry,
            Some(tx_executor_network),
        );
        handles.extend(primary_handles);

        Ok(handles)
    }

    /// Spawn the consensus core and the client executing transactions.
    async fn spawn_consensus<State>(
        name: PublicKey,
        rx_executor_network: oneshot::Receiver<anemo::Network>,
        worker_cache: SharedWorkerCache,
        committee: SharedCommittee,
        store: &NodeStorage,
        parameters: Parameters,
        execution_state: State,
        mut shutdown_receivers: Vec<ConditionalBroadcastReceiver>,
        rx_new_certificates: metered_channel::Receiver<Certificate>,
        tx_committed_certificates: metered_channel::Sender<(Round, Vec<Certificate>)>,
        tx_consensus_round_updates: watch::Sender<Round>,
        registry: &Registry,
    ) -> SubscriberResult<Vec<JoinHandle<()>>>
    where
        PublicKey: VerifyingKey,
        State: ExecutionState + Send + Sync + 'static,
    {
        let consensus_metrics = Arc::new(ConsensusMetrics::new(registry));
        let channel_metrics = ChannelMetrics::new(registry);

        let (tx_sequence, rx_sequence) =
            metered_channel::channel(Self::CHANNEL_CAPACITY, &channel_metrics.tx_sequence);

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
        consensus_metrics
            .recovered_consensus_output
            .inc_by(num_sub_dags);

        // Spawn the consensus core who only sequences transactions.
        let ordering_engine = Bullshark::new(
            (**committee.load()).clone(),
            store.consensus_store.clone(),
            parameters.gc_depth,
            consensus_metrics.clone(),
        );
        let consensus_handles = Consensus::spawn(
            (**committee.load()).clone(),
            store.consensus_store.clone(),
            store.certificate_store.clone(),
            shutdown_receivers.pop().unwrap(),
            rx_new_certificates,
            tx_committed_certificates,
            tx_consensus_round_updates,
            tx_sequence,
            ordering_engine,
            consensus_metrics.clone(),
        );

        // Spawn the client executing the transactions. It can also synchronize with the
        // subscriber handler if it missed some transactions.
        let executor_handles = Executor::spawn(
            name,
            rx_executor_network,
            worker_cache,
            (**committee.load()).clone(),
            execution_state,
            shutdown_receivers,
            rx_sequence,
            registry,
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
    pub fn new(
        parameters: Parameters,
        internal_consensus: bool,
        registry_service: RegistryService,
    ) -> PrimaryNode {
        let inner = PrimaryNodeInner {
            parameters,
            internal_consensus,
            registry_service,
            registry: None,
            handles: FuturesUnordered::new(),
            tx_shutdown: None,
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
        committee: SharedCommittee,
        // The worker information cache.
        worker_cache: SharedWorkerCache,
        // The node's store //TODO: replace this by a path so the method can open and independent storage
        store: &NodeStorage,
        // The state used by the client to execute transactions.
        execution_state: Arc<State>,
    ) -> Result<(), NodeError>
    where
        State: ExecutionState + Send + Sync + 'static,
    {
        let mut guard = self.internal.write().await;
        guard
            .start(
                keypair,
                network_keypair,
                committee,
                worker_cache,
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

    pub async fn registry(&self) -> Option<(RegistryID, Registry)> {
        let guard = self.internal.read().await;
        guard.registry.clone()
    }
}
