// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::GrpcMetrics;
use anemo::Network;
use anemo_tower::callback::CallbackLayer;
use anemo_tower::trace::DefaultMakeSpan;
use anemo_tower::trace::DefaultOnFailure;
use anemo_tower::trace::TraceLayer;
use anyhow::anyhow;
use anyhow::Result;
use checkpoint_executor::CheckpointExecutor;
use futures::TryFutureExt;
use mysten_metrics::{spawn_monitored_task, RegistryService};
use mysten_network::server::ServerBuilder;
use narwhal_network::metrics::MetricsMakeCallbackHandler;
use narwhal_network::metrics::{NetworkConnectionMetrics, NetworkMetrics};
use prometheus::Registry;
use std::collections::HashMap;
use std::sync::Arc;
use sui_config::{ConsensusConfig, NodeConfig};
use sui_core::authority_aggregator::{
    AuthAggMetrics, AuthorityAggregator, NetworkTransactionCertifier,
};
use sui_core::authority_server::ValidatorService;
use sui_core::checkpoints::checkpoint_executor;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_core::safe_client::SafeClientMetricsBase;
use sui_core::storage::RocksDbStore;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_core::transaction_streamer::TransactionStreamer;
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_client::NetworkAuthorityClient,
};
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::event_api::EventReadApiImpl;
use sui_json_rpc::event_api::EventStreamingApiImpl;
use sui_json_rpc::read_api::FullNodeApi;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::streaming_api::TransactionStreamingApiImpl;
use sui_json_rpc::transaction_builder_api::FullNodeTransactionBuilderApi;
use sui_json_rpc::transaction_execution_api::FullNodeTransactionExecutionApi;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use sui_network::api::ValidatorServer;
use sui_network::discovery;
use sui_network::state_sync;
use sui_storage::{
    event_store::{EventStoreType, SqlEventStore},
    IndexStore,
};
use sui_types::committee::Committee;
use sui_types::crypto::KeypairTraits;
use sui_types::messages::QuorumDriverResponse;
use tokio::sync::mpsc::channel;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tower::ServiceBuilder;
use tracing::info;
use typed_store::DBMetrics;
pub mod admin;
mod handle;
pub mod metrics;
pub use handle::SuiNodeHandle;
use narwhal_config::SharedWorkerCache;
use narwhal_types::TransactionsClient;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::checkpoints::{
    CheckpointMetrics, CheckpointService, CheckpointStore, SendCheckpointToStateSync,
    SubmitCheckpointToConsensus,
};
use sui_core::consensus_adapter::{ConsensusAdapter, ConsensusAdapterMetrics};
use sui_core::consensus_handler::ConsensusHandler;
use sui_core::consensus_validator::SuiTxValidator;
use sui_core::epoch::reconfiguration::ReconfigurationInitiator;
use sui_core::narwhal_manager::{
    run_narwhal_manager, NarwhalConfiguration, NarwhalManager, NarwhalStartMessage,
};
use sui_json_rpc::coin_api::CoinReadApi;
use sui_types::base_types::{EpochId, TransactionDigest};
use sui_types::error::{SuiError, SuiResult};

pub struct ValidatorComponents {
    validator_server_handle: tokio::task::JoinHandle<Result<()>>,
    narwhal_manager: NarwhalManager<ConsensusHandler<Arc<AuthorityStore>>>,
    consensus_adapter: Arc<ConsensusAdapter>,
    // dropping this will eventually stop checkpoint tasks. The receiver side of this channel
    // is copied into each checkpoint service task, and they are listening to any change to this
    // channel. When the sender is dropped, a change is triggered and those tasks will exit.
    checkpoint_service_exit: watch::Sender<()>,
    checkpoint_metrics: Arc<CheckpointMetrics>,
}

pub struct SuiNode {
    config: NodeConfig,
    validator_components: Mutex<Option<ValidatorComponents>>,
    _json_rpc_service: Option<ServerHandle>,
    state: Arc<AuthorityState>,
    transaction_orchestrator: Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    registry_service: RegistryService,

    _p2p_network: Network,
    _discovery: discovery::Handle,
    state_sync: state_sync::Handle,
    checkpoint_store: Arc<CheckpointStore>,
    checkpoint_executor_handle: checkpoint_executor::Handle,

    reconfig_channel: Mutex<tokio::sync::broadcast::Receiver<Committee>>,

    #[cfg(msim)]
    sim_node: sui_simulator::runtime::NodeHandle,
}

impl SuiNode {
    pub async fn start(
        config: &NodeConfig,
        registry_service: RegistryService,
    ) -> Result<Arc<SuiNode>> {
        // TODO: maybe have a config enum that takes care of this for us.
        let is_validator = config.consensus_config().is_some();
        let is_full_node = !is_validator;
        let prometheus_registry = registry_service.default_registry();

        info!(node =? config.protocol_public_key(),
            "Initializing sui-node listening on {}", config.network_address
        );

        // Initialize metrics to track db usage before creating any stores
        DBMetrics::init(&prometheus_registry);
        mysten_metrics::init_metrics(&prometheus_registry);

        let genesis = config.genesis()?;

        let secret = Arc::pin(config.protocol_key_pair().copy());
        let genesis_committee = genesis.committee()?;
        let committee_store = Arc::new(CommitteeStore::new(
            config.db_path().join("epochs"),
            &genesis_committee,
            None,
        ));
        let store = Arc::new(
            AuthorityStore::open(
                &config.db_path().join("store"),
                None,
                genesis,
                &committee_store,
                &config.authority_store_pruning_config,
            )
            .await?,
        );

        let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));
        let state_sync_store = RocksDbStore::new(
            store.clone(),
            committee_store.clone(),
            checkpoint_store.clone(),
        );

        let index_store = if is_validator {
            None
        } else {
            Some(Arc::new(IndexStore::new(config.db_path().join("indexes"))))
        };

        let event_store = if config.enable_event_processing {
            let path = config.db_path().join("events.db");
            let db = SqlEventStore::new_from_file(&path).await?;
            db.initialize().await?;

            if index_store.is_none() {
                return Err(anyhow!(
                    "event storage requires that IndexStore be enabled as well"
                ));
            }

            Some(Arc::new(EventStoreType::SqlEventStore(db)))
        } else {
            None
        };

        let (p2p_network, discovery_handle, state_sync_handle) =
            Self::create_p2p_network(config, state_sync_store, &prometheus_registry)?;

        let arc_net = AuthorityAggregator::new_from_system_state(
            &store,
            &committee_store,
            SafeClientMetricsBase::new(&prometheus_registry),
            AuthAggMetrics::new(&prometheus_registry),
        )?;

        let transaction_streamer = if is_full_node {
            Some(Arc::new(TransactionStreamer::new()))
        } else {
            None
        };

        let state = AuthorityState::new(
            config.protocol_public_key(),
            secret,
            store,
            committee_store.clone(),
            index_store.clone(),
            event_store,
            transaction_streamer,
            &prometheus_registry,
        )
        .await;

        let (checkpoint_executor_handle, reconfig_channel) = CheckpointExecutor::new(
            state_sync_handle.subscribe_to_synced_checkpoints(),
            checkpoint_store.clone(),
            state.clone(),
            config.checkpoint_executor_config.clone(),
            &prometheus_registry,
        )
        .start()?;

        let transaction_orchestrator = if is_full_node {
            Some(Arc::new(TransactiondOrchestrator::new(
                Arc::new(arc_net),
                state.clone(),
                config.db_path(),
                &prometheus_registry,
            )))
        } else {
            None
        };

        let json_rpc_service = build_server(
            state.clone(),
            &transaction_orchestrator.clone(),
            config,
            &prometheus_registry,
        )
        .await?;

        let validator_components = if state.is_validator() {
            Some(
                Self::construct_validator_components(
                    config,
                    state.clone(),
                    checkpoint_store.clone(),
                    state_sync_handle.clone(),
                    &registry_service,
                )
                .await?,
            )
        } else {
            None
        };

        let node = Self {
            config: config.clone(),
            validator_components: Mutex::new(validator_components),
            _json_rpc_service: json_rpc_service,
            state,
            transaction_orchestrator,
            registry_service,

            _p2p_network: p2p_network,
            _discovery: discovery_handle,
            state_sync: state_sync_handle,
            checkpoint_store,
            checkpoint_executor_handle,
            reconfig_channel: Mutex::new(reconfig_channel),

            #[cfg(msim)]
            sim_node: sui_simulator::runtime::NodeHandle::current(),
        };

        info!("SuiNode started!");
        let node = Arc::new(node);
        let node_copy = node.clone();
        spawn_monitored_task!(async move { Self::monitor_reconfiguration(node_copy).await });

        Ok(node)
    }

    pub async fn subscribe_to_epoch_change(&self) -> tokio::sync::broadcast::Receiver<Committee> {
        self.checkpoint_executor_handle.subscribe_to_end_of_epoch()
    }

    pub fn current_epoch(&self) -> EpochId {
        self.state.epoch()
    }

    pub async fn close_epoch(&self) -> SuiResult {
        self.validator_components
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| SuiError::from("Node is not a validator"))?
            .consensus_adapter
            // TODO: If this function is ever called in non-testing code, we need to make sure this
            // function has no race with the passive reconfiguration.
            .close_epoch(&self.state.epoch_store())
    }

    pub fn tx_checkpointed_in_current_epoch(&self, digest: &TransactionDigest) -> SuiResult<bool> {
        Ok(self
            .state
            .epoch_store()
            .tx_checkpointed_in_current_epoch(digest)?)
    }

    fn create_p2p_network(
        config: &NodeConfig,
        state_sync_store: RocksDbStore,
        prometheus_registry: &Registry,
    ) -> Result<(Network, discovery::Handle, state_sync::Handle)> {
        let (state_sync, state_sync_server) = state_sync::Builder::new()
            .config(config.p2p_config.state_sync.clone().unwrap_or_default())
            .store(state_sync_store)
            .with_metrics(prometheus_registry)
            .build();

        // TODO only configure validators as seed/preferred peers for validators and not for
        // fullnodes once we've had a chance to re-work fullnode configuration generation.
        let mut p2p_config = config.p2p_config.clone();
        let network_kp = config.network_key_pair();
        let our_network_public_key = network_kp.public();
        let other_validators = config
            .genesis()?
            .validator_set()
            .iter()
            .filter(|validator| &validator.network_key != our_network_public_key)
            .map(|validator| sui_config::p2p::SeedPeer {
                peer_id: Some(anemo::PeerId(validator.network_key.0.to_bytes())),
                address: validator.p2p_address.clone(),
            });
        p2p_config.seed_peers.extend(other_validators);

        let (discovery, discovery_server) = discovery::Builder::new().config(p2p_config).build();

        let p2p_network = {
            let routes = anemo::Router::new()
                .add_rpc_service(discovery_server)
                .add_rpc_service(state_sync_server);

            let inbound_network_metrics =
                NetworkMetrics::new("sui", "inbound", prometheus_registry);
            let outbound_network_metrics =
                NetworkMetrics::new("sui", "outbound", prometheus_registry);
            let network_connection_metrics =
                NetworkConnectionMetrics::new("sui", prometheus_registry);

            let service = ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_server_errors()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
                )
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(inbound_network_metrics),
                )))
                .service(routes);

            let outbound_layer = ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_client_and_server_errors()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO))
                        .on_failure(DefaultOnFailure::new().level(tracing::Level::WARN)),
                )
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(outbound_network_metrics),
                )))
                .into_inner();

            let network = Network::bind(config.p2p_config.listen_address)
                .server_name("sui")
                .private_key(config.network_key_pair().copy().private().0.to_bytes())
                .config(config.p2p_config.anemo_config.clone().unwrap_or_default())
                .outbound_request_layer(outbound_layer)
                .start(service)?;
            info!("P2p network started on {}", network.local_addr());

            let _connection_monitor_handle =
                narwhal_network::connectivity::ConnectionMonitor::spawn(
                    network.downgrade(),
                    network_connection_metrics,
                    HashMap::default(),
                );

            network
        };

        let discovery_handle = discovery.start(p2p_network.clone());
        let state_sync_handle = state_sync.start(p2p_network.clone());
        Ok((p2p_network, discovery_handle, state_sync_handle))
    }

    async fn construct_validator_components(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        checkpoint_store: Arc<CheckpointStore>,
        state_sync_handle: state_sync::Handle,
        registry_service: &RegistryService,
    ) -> Result<ValidatorComponents> {
        let consensus_config = config
            .consensus_config()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?;

        let consensus_adapter = Self::construct_consensus_adapter(
            consensus_config,
            state.clone(),
            &registry_service.default_registry(),
        );

        let validator_server_handle = Self::start_grpc_validator_service(
            config,
            state.clone(),
            consensus_adapter.clone(),
            &registry_service.default_registry(),
        )
        .await?;

        let narwhal_manager = Self::construct_and_run_narwhal_manager(
            config,
            consensus_config,
            state.clone(),
            registry_service,
        )?;

        let checkpoint_metrics = CheckpointMetrics::new(&registry_service.default_registry());
        Self::start_epoch_specific_validator_components(
            config,
            state.clone(),
            consensus_adapter,
            checkpoint_store,
            state.epoch_store().clone(),
            state_sync_handle,
            narwhal_manager,
            validator_server_handle,
            checkpoint_metrics,
        )
        .await
    }

    async fn start_epoch_specific_validator_components(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        checkpoint_store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        state_sync_handle: state_sync::Handle,
        narwhal_manager: NarwhalManager<ConsensusHandler<Arc<AuthorityStore>>>,
        validator_server_handle: JoinHandle<Result<()>>,
        checkpoint_metrics: Arc<CheckpointMetrics>,
    ) -> Result<ValidatorComponents> {
        let (checkpoint_service, checkpoint_service_exit) = Self::start_checkpoint_service(
            config,
            consensus_adapter.clone(),
            checkpoint_store,
            epoch_store.clone(),
            state.clone(),
            state_sync_handle,
            checkpoint_metrics.clone(),
        );

        let consensus_handler = Arc::new(ConsensusHandler::new(
            epoch_store,
            checkpoint_service.clone(),
            state.transaction_manager().clone(),
            state.db(),
            state.metrics.clone(),
        ));

        let system_state = state
            .get_sui_system_state_object()
            .expect("Reading Sui system state object cannot fail");
        let committee = Arc::new(system_state.get_current_epoch_narwhal_committee());

        let transactions_addr = &config
            .consensus_config
            .as_ref()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?
            .address;
        let worker_cache = system_state.get_current_epoch_narwhal_worker_cache(transactions_addr);

        let msg = NarwhalStartMessage {
            committee: committee.clone(),
            shared_worker_cache: SharedWorkerCache::from(worker_cache),
            execution_state: consensus_handler,
        };
        info!("Sending start signal to Narwhal");
        narwhal_manager.tx_start.send(msg).await?;
        // TODO: (Laura) wait for start complete signal

        Ok(ValidatorComponents {
            validator_server_handle,
            narwhal_manager,
            consensus_adapter,
            checkpoint_service_exit,
            checkpoint_metrics,
        })
    }

    fn start_checkpoint_service(
        config: &NodeConfig,
        consensus_adapter: Arc<ConsensusAdapter>,
        checkpoint_store: Arc<CheckpointStore>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        state: Arc<AuthorityState>,
        state_sync_handle: state_sync::Handle,
        checkpoint_metrics: Arc<CheckpointMetrics>,
    ) -> (Arc<CheckpointService>, watch::Sender<()>) {
        let checkpoint_output = Box::new(SubmitCheckpointToConsensus {
            sender: consensus_adapter,
            signer: state.secret.clone(),
            authority: config.protocol_public_key(),
            checkpoints_per_epoch: config.checkpoints_per_epoch,
        });

        let certified_checkpoint_output = SendCheckpointToStateSync::new(state_sync_handle);

        CheckpointService::spawn(
            state.clone(),
            checkpoint_store,
            epoch_store,
            Box::new(state.db()),
            checkpoint_output,
            Box::new(certified_checkpoint_output),
            Box::new(NetworkTransactionCertifier::default()),
            checkpoint_metrics,
        )
    }

    fn construct_and_run_narwhal_manager(
        config: &NodeConfig,
        consensus_config: &ConsensusConfig,
        state: Arc<AuthorityState>,
        registry_service: &RegistryService,
    ) -> Result<NarwhalManager<ConsensusHandler<Arc<AuthorityStore>>>> {
        let narwhal_config = NarwhalConfiguration {
            primary_keypair: config.protocol_key_pair().copy(),
            network_keypair: config.network_key_pair().copy(),
            worker_ids_and_keypairs: vec![(0, config.worker_key_pair().copy())],
            storage_base_path: consensus_config.db_path().to_path_buf(),
            parameters: consensus_config.narwhal_config().to_owned(),
            tx_validator: SuiTxValidator::new(state, &registry_service.default_registry()),
            registry_service: registry_service.clone(),
        };

        let (tx_start, tr_start) = channel(1);
        let (tx_stop, tr_stop) = channel(1);
        let join_handle =
            spawn_monitored_task!(run_narwhal_manager(narwhal_config, tr_start, tr_stop));

        let narwhal_manager = NarwhalManager {
            join_handle,
            tx_start,
            tx_stop,
        };

        Ok(narwhal_manager)
    }

    fn construct_consensus_adapter(
        consensus_config: &ConsensusConfig,
        state: Arc<AuthorityState>,
        prometheus_registry: &Registry,
    ) -> Arc<ConsensusAdapter> {
        let consensus_address = consensus_config.address().to_owned();
        let consensus_client = TransactionsClient::new(
            mysten_network::client::connect_lazy(&consensus_address)
                .expect("Failed to connect to consensus"),
        );

        let ca_metrics = ConsensusAdapterMetrics::new(prometheus_registry);
        // The consensus adapter allows the authority to send user certificates through consensus.

        ConsensusAdapter::new(Box::new(consensus_client), state, ca_metrics)
    }

    async fn start_grpc_validator_service(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        consensus_adapter: Arc<ConsensusAdapter>,
        prometheus_registry: &Registry,
    ) -> Result<tokio::task::JoinHandle<Result<()>>> {
        let validator_service =
            ValidatorService::new(state.clone(), consensus_adapter, prometheus_registry).await?;

        let mut server_conf = mysten_network::config::Config::new();
        server_conf.global_concurrency_limit = config.grpc_concurrency_limit;
        server_conf.load_shed = config.grpc_load_shed;
        let mut server_builder =
            ServerBuilder::from_config(&server_conf, GrpcMetrics::new(prometheus_registry));

        server_builder = server_builder.add_service(ValidatorServer::new(validator_service));

        let server = server_builder
            .bind(config.network_address())
            .await
            .map_err(|err| anyhow!(err.to_string()))?;
        let local_addr = server.local_addr();
        info!("Listening to traffic on {local_addr}");
        let grpc_server = spawn_monitored_task!(server.serve().map_err(Into::into));

        Ok(grpc_server)
    }

    pub fn state(&self) -> Arc<AuthorityState> {
        self.state.clone()
    }

    pub fn clone_committee_store(&self) -> Arc<CommitteeStore> {
        self.state.committee_store().clone()
    }

    pub fn clone_authority_store(&self) -> Arc<AuthorityStore> {
        self.state.db()
    }

    /// Clone an AuthorityAggregator currently used in this node's
    /// QuorumDriver, if the node is a fullnode. After reconfig,
    /// QuorumDrvier builds a new AuthorityAggregator. The caller
    /// of this function will mostly likely want to call this again
    /// to get a fresh one.
    pub fn clone_authority_aggregator(
        &self,
    ) -> Option<Arc<AuthorityAggregator<NetworkAuthorityClient>>> {
        self.transaction_orchestrator
            .as_ref()
            .map(|to| to.clone_authority_aggregator())
    }

    pub fn transaction_orchestrator(
        &self,
    ) -> Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>> {
        self.transaction_orchestrator.clone()
    }

    pub fn subscribe_to_transaction_orchestrator_effects(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<QuorumDriverResponse>> {
        self.transaction_orchestrator
            .as_ref()
            .map(|to| to.subscribe_to_effects_queue())
            .ok_or_else(|| anyhow::anyhow!("Transaction Orchestrator is not enabled in this node."))
    }

    /// This function waits for a signal from the checkpoint executor to indicate that on-chain
    /// epoch has changed. Upon receiving such signal, we reconfigure the entire system.
    pub async fn monitor_reconfiguration(self: Arc<Self>) -> Result<()> {
        loop {
            let Committee {
                epoch: next_epoch, ..
            } = self
                .reconfig_channel
                .lock()
                .await
                .recv()
                .await
                .expect("Reconfiguration channel was closed unexpectedly.");
            info!(
                ?next_epoch,
                "Received reconfiguration signal. About to reconfigure the system."
            );

            // The following code handles 4 different cases, depending on whether the node
            // was a validator in the previous epoch, and whether the node is a validator
            // in the new epoch.
            let new_validator_components = if let Some(ValidatorComponents {
                validator_server_handle,
                narwhal_manager,
                consensus_adapter,
                checkpoint_service_exit,
                checkpoint_metrics,
            }) = self.validator_components.lock().await.take()
            {
                info!("Reconfiguring the validator.");
                // Stop the old checkpoint service.
                drop(checkpoint_service_exit);

                info!("Sending shutdown signal to Narwhal");
                narwhal_manager.tx_stop.send(()).await?;
                // TODO: (Laura) wait for stop complete signal

                self.reconfigure_state(next_epoch).await;

                if self.state.is_validator() {
                    // Only restart Narwhal if this node is still a validator in the new epoch.
                    Some(
                        Self::start_epoch_specific_validator_components(
                            &self.config,
                            self.state.clone(),
                            consensus_adapter,
                            self.checkpoint_store.clone(),
                            self.state.epoch_store().clone(),
                            self.state_sync.clone(),
                            narwhal_manager,
                            validator_server_handle,
                            checkpoint_metrics,
                        )
                        .await?,
                    )
                } else {
                    info!("This node is no longer a validator after reconfiguration");
                    None
                }
            } else {
                self.reconfigure_state(next_epoch).await;

                if self.state.is_validator() {
                    info!("Promoting the node from fullnode to validator, starting grpc server");

                    Some(
                        Self::construct_validator_components(
                            &self.config,
                            self.state.clone(),
                            self.checkpoint_store.clone(),
                            self.state_sync.clone(),
                            &self.registry_service,
                        )
                        .await?,
                    )
                } else {
                    None
                }
            };
            *self.validator_components.lock().await = new_validator_components;
            info!("Reconfiguration finished");
        }
    }

    async fn reconfigure_state(&self, next_epoch: EpochId) {
        let system_state = self
            .state
            .get_sui_system_state_object()
            .expect("Reading Sui system state object cannot fail");
        let new_committee = system_state.get_current_epoch_committee();
        assert_eq!(next_epoch, new_committee.committee.epoch);
        self.state
            .reconfigure(new_committee.committee)
            .await
            .expect("Reconfigure authority state cannot fail");
        info!("Validator State has been reconfigured");
    }
}

pub async fn build_server(
    state: Arc<AuthorityState>,
    transaction_orchestrator: &Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    config: &NodeConfig,
    prometheus_registry: &Registry,
) -> Result<Option<ServerHandle>> {
    // Validators do not expose these APIs
    if config.consensus_config().is_some() {
        return Ok(None);
    }

    let mut server = JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry)?;

    server.register_module(ReadApi::new(state.clone()))?;
    server.register_module(CoinReadApi::new(state.clone()))?;
    server.register_module(FullNodeApi::new(state.clone()))?;
    server.register_module(BcsApiImpl::new(state.clone()))?;
    server.register_module(FullNodeTransactionBuilderApi::new(state.clone()))?;

    if let Some(transaction_orchestrator) = transaction_orchestrator {
        server.register_module(FullNodeTransactionExecutionApi::new(
            transaction_orchestrator.clone(),
            state.module_cache.clone(),
        ))?;
    }

    if let Some(event_handler) = state.event_handler.clone() {
        server.register_module(EventReadApiImpl::new(state.clone(), event_handler))?;
    }

    if let Some(tx_streamer) = state.transaction_streamer.clone() {
        server.register_module(TransactionStreamingApiImpl::new(state.clone(), tx_streamer))?;
    }

    if let Some(event_handler) = state.event_handler.clone() {
        server.register_module(EventStreamingApiImpl::new(state.clone(), event_handler))?;
    }

    let rpc_server_handle = server.start(config.json_rpc_address).await?;

    Ok(Some(rpc_server_handle))
}
