// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::Network;
use anemo_tower::callback::CallbackLayer;
use anemo_tower::trace::DefaultMakeSpan;
use anemo_tower::trace::TraceLayer;
use anyhow::anyhow;
use anyhow::Result;
use futures::TryFutureExt;
use mysten_network::server::ServerBuilder;
use narwhal_network::metrics::MetricsMakeCallbackHandler;
use narwhal_network::metrics::{NetworkConnectionMetrics, NetworkMetrics};
use prometheus::Registry;
use std::collections::HashMap;
use std::option::Option::None;
use std::{sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_server::ValidatorService;
use sui_core::checkpoints::checkpoint_executor::CheckpointExecutor;
use sui_core::storage::RocksDbStore;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_core::transaction_streamer::TransactionStreamer;
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_active::ActiveAuthority,
    authority_client::NetworkAuthorityClient,
};
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::streaming_api::TransactionStreamingApiImpl;
use sui_json_rpc::transaction_builder_api::FullNodeTransactionBuilderApi;
use sui_network::api::ValidatorServer;
use sui_network::discovery;
use sui_network::state_sync;
use sui_storage::{
    event_store::{EventStoreType, SqlEventStore},
    node_sync_store::NodeSyncStore,
    IndexStore,
};
use sui_types::messages::{CertifiedTransaction, CertifiedTransactionEffects};
use tokio::sync::mpsc::channel;
use tower::ServiceBuilder;
use tracing::info;
use typed_store::DBMetrics;

use crate::metrics::GrpcMetrics;
use mysten_metrics::spawn_monitored_task;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_json_rpc::event_api::EventReadApiImpl;
use sui_json_rpc::event_api::EventStreamingApiImpl;
use sui_json_rpc::read_api::FullNodeApi;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::transaction_execution_api::FullNodeTransactionExecutionApi;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use sui_types::crypto::KeypairTraits;

pub mod admin;
pub mod metrics;

mod handle;
pub use handle::SuiNodeHandle;
use sui_core::authority::ReconfigConsensusMessage;
use sui_core::checkpoints::CheckpointStore;
use sui_json_rpc::coin_api::CoinReadApi;
use sui_types::committee::EpochId;

type ValidatorServerInfo = (
    tokio::task::JoinHandle<Result<()>>,
    tokio::sync::mpsc::Sender<ReconfigConsensusMessage>,
);

pub struct SuiNode {
    config: NodeConfig,
    checkpoint_store: Arc<CheckpointStore>,
    validator_server_info: Option<ValidatorServerInfo>,
    _json_rpc_service: Option<ServerHandle>,
    _batch_subsystem_handle: tokio::task::JoinHandle<()>,
    _post_processing_subsystem_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    _gossip_handle: Option<tokio::task::JoinHandle<()>>,
    state: Arc<AuthorityState>,
    active: Arc<ActiveAuthority<NetworkAuthorityClient>>,
    transaction_orchestrator: Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    _prometheus_registry: Registry,

    _p2p_network: anemo::Network,
    _discovery: discovery::Handle,
    _state_sync: state_sync::Handle,
    _checkpoint_executor_handle: tokio::task::JoinHandle<()>,

    reconfig_channel: (
        tokio::sync::mpsc::Sender<EpochId>,
        tokio::sync::mpsc::Receiver<EpochId>,
    ),

    #[cfg(msim)]
    sim_node: sui_simulator::runtime::NodeHandle,
}

impl SuiNode {
    pub async fn start(config: &NodeConfig, prometheus_registry: Registry) -> Result<SuiNode> {
        // TODO: maybe have a config enum that takes care of this for us.
        let is_validator = config.consensus_config().is_some();
        let is_full_node = !is_validator;

        info!(node =? config.protocol_public_key(),
            "Initializing sui-node listening on {}", config.network_address
        );

        // Initialize metrics to track db usage before creating any stores
        DBMetrics::init(&prometheus_registry);
        mysten_metrics::init_metrics(&prometheus_registry);

        let genesis = config.genesis()?;

        let secret = Arc::pin(config.protocol_key_pair().copy());
        let committee = genesis.committee()?;
        let store =
            Arc::new(AuthorityStore::open(&config.db_path().join("store"), None, genesis).await?);
        let committee_store = Arc::new(CommitteeStore::new(
            config.db_path().join("epochs"),
            &committee,
            None,
        ));
        let checkpoint_store = CheckpointStore::new(&config.db_path().join("checkpoints"));

        let state_sync_store = RocksDbStore::new(
            store.clone(),
            committee_store.clone(),
            checkpoint_store.clone(),
        );

        let index_store = if is_validator {
            None
        } else {
            Some(Arc::new(IndexStore::open_tables_read_write(
                config.db_path().join("indexes"),
                None,
                None,
            )))
        };

        let event_store = if config.enable_event_processing {
            let path = config.db_path().join("events.db");
            let db = SqlEventStore::new_from_file(&path).await?;
            db.initialize().await?;
            Some(Arc::new(EventStoreType::SqlEventStore(db)))
        } else {
            None
        };

        let (p2p_network, discovery_handle, state_sync_handle) =
            Self::create_p2p_network(config, state_sync_store, &prometheus_registry)?;

        let net = AuthorityAggregator::new_from_system_state(
            &store,
            &committee_store,
            &prometheus_registry,
        )?;

        let reconfig_channel = channel(1);

        let transaction_streamer = if is_full_node {
            Some(Arc::new(TransactionStreamer::new()))
        } else {
            None
        };

        let node_sync_store = Arc::new(NodeSyncStore::open_tables_read_write(
            config.db_path().join("node_sync_db"),
            None,
            None,
        ));

        let state = AuthorityState::new(
            config.protocol_public_key(),
            secret,
            store,
            node_sync_store,
            committee_store.clone(),
            index_store.clone(),
            event_store,
            transaction_streamer,
            &prometheus_registry,
        )
        .await;

        let checkpoint_executor_handle = {
            let executor = CheckpointExecutor::new(
                state_sync_handle.subscribe_to_synced_checkpoints(),
                checkpoint_store.clone(),
                state.clone(),
                &prometheus_registry,
            )?;
            tokio::spawn(executor.run())
        };

        let active_authority = Arc::new(ActiveAuthority::new(
            state.clone(),
            net.clone(),
            &prometheus_registry,
        )?);

        let arc_net = active_authority.agg_aggregator();

        let transaction_orchestrator = if is_full_node {
            Some(Arc::new(TransactiondOrchestrator::new(
                arc_net,
                state.clone(),
                &prometheus_registry,
            )))
        } else {
            None
        };

        let batch_subsystem_handle = {
            // Start batch system so that this node can be followed
            let batch_state = state.clone();
            spawn_monitored_task!(async move {
                batch_state
                    .run_batch_service(1000, Duration::from_secs(1))
                    .await
            })
        };

        let post_processing_subsystem_handle =
            if index_store.is_some() || config.enable_event_processing {
                let indexing_state = state.clone();
                Some(spawn_monitored_task!(async move {
                    indexing_state
                        .run_tx_post_processing_process()
                        .await
                        .map_err(Into::into)
                }))
            } else {
                None
            };

        let gossip_handle = if is_full_node {
            active_authority.clone().spawn_node_sync_process().await;
            None
        } else {
            None
        };

        let validator_server_info = Self::start_grpc_validator_service(
            config,
            state.clone(),
            checkpoint_store.clone(),
            &state_sync_handle,
            &prometheus_registry,
        )
        .await?;

        let json_rpc_service = build_server(
            state.clone(),
            &transaction_orchestrator.clone(),
            config,
            &prometheus_registry,
        )
        .await?;

        let node = Self {
            config: config.clone(),
            checkpoint_store,
            validator_server_info,
            _json_rpc_service: json_rpc_service,
            _gossip_handle: gossip_handle,
            _batch_subsystem_handle: batch_subsystem_handle,
            _post_processing_subsystem_handle: post_processing_subsystem_handle,
            state,
            active: active_authority,
            transaction_orchestrator,
            _prometheus_registry: prometheus_registry,
            _p2p_network: p2p_network,
            _discovery: discovery_handle,
            _state_sync: state_sync_handle,
            _checkpoint_executor_handle: checkpoint_executor_handle,
            reconfig_channel,

            #[cfg(msim)]
            sim_node: sui_simulator::runtime::NodeHandle::current(),
        };

        info!("SuiNode started!");

        Ok(node)
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
        let our_network_public_key = config.network_key_pair.public();
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
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
                )
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(inbound_network_metrics),
                )))
                .service(routes);

            let outbound_layer = ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_client_and_server_errors()
                        .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
                )
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(outbound_network_metrics),
                )))
                .into_inner();

            let network = Network::bind(config.p2p_config.listen_address)
                .server_name("sui")
                .private_key(config.network_key_pair.copy().private().0.to_bytes())
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

    async fn start_grpc_validator_service(
        config: &NodeConfig,
        state: Arc<AuthorityState>,
        checkpoint_store: Arc<CheckpointStore>,
        state_sync_handle: &state_sync::Handle,
        prometheus_registry: &Registry,
    ) -> Result<Option<ValidatorServerInfo>> {
        if state.is_fullnode() {
            return Ok(None);
        }

        let (tx_reconfigure_consensus, rx_reconfigure_consensus) = channel(100);

        let validator_service = ValidatorService::new(
            config,
            state.clone(),
            checkpoint_store,
            state_sync_handle.clone(),
            prometheus_registry.clone(),
            rx_reconfigure_consensus,
        )
        .await?;

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

        Ok(Some((grpc_server, tx_reconfigure_consensus)))
    }

    pub fn state(&self) -> Arc<AuthorityState> {
        self.state.clone()
    }

    pub fn active(&self) -> &Arc<ActiveAuthority<NetworkAuthorityClient>> {
        &self.active
    }

    pub fn transaction_orchestrator(
        &self,
    ) -> Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>> {
        self.transaction_orchestrator.clone()
    }

    pub fn subscribe_to_transaction_orchestrator_effects(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<(CertifiedTransaction, CertifiedTransactionEffects)>>
    {
        self.transaction_orchestrator
            .as_ref()
            .map(|to| to.subscribe_to_effects_queue())
            .ok_or_else(|| anyhow::anyhow!("Transaction Orchestrator is not enabled in this node."))
    }

    /// This function waits for a signal from the checkpoint executor to indicate that on-chain
    /// epoch has changed. Upon receiving such signal, we reconfigure the entire system.
    pub async fn monitor_reconfiguration(mut self) -> Result<()> {
        loop {
            let next_epoch = self
                .reconfig_channel
                .1
                .recv()
                .await
                .expect("Reconfiguration channel was closed unexpectedly.");
            info!(
                ?next_epoch,
                "Received reconfiguration signal. About to reconfigure the system."
            );
            let new_committee = self
                .state
                .get_sui_system_state_object()
                .await
                .expect("Reading Sui system state object cannot fail")
                .get_current_epoch_committee();
            assert_eq!(next_epoch, new_committee.committee.epoch);
            let was_validator = self.state.is_validator();
            assert_eq!(was_validator, self.validator_server_info.is_some());
            if was_validator {
                info!("Reconfiguring the validator.");
                info!("Shutting down Narwhal");
                // TODO: Shutdown Narwhal here.
            }

            self.state
                .reconfigure(new_committee.committee)
                .expect("Reconfigure authority state cannot fail");
            info!("Validator State has been reconfigured");

            if self.state.is_validator() {
                // If the node is a validator in the new epoch, we need to make sure validator
                // service is running in the new epoch.
                if was_validator {
                    // If we were a validator in previous epoch, we don't need to start grpc server.
                    // Only need to start Narwhal.
                    // TODO: Start Narwhal here.
                    info!("Starting Narwhal");
                } else {
                    // TODO: It might be easier to require node operator to manually restart the node
                    // for this transition.
                    info!("Promoting the node from fullnode to validator, starting grpc server");
                    self.validator_server_info = Self::start_grpc_validator_service(
                        &self.config,
                        self.state.clone(),
                        self.checkpoint_store.clone(),
                        &self._state_sync,
                        &self._prometheus_registry,
                    )
                    .await
                    .expect("Starting grpc server cannot fail");
                }
            } else {
                info!("This node is no longer a validator after reconfiguration");
            }
            info!("Reconfiguration finished");
        }
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
