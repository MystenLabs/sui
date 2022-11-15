// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo_tower::callback::CallbackLayer;
use anemo_tower::trace::TraceLayer;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use futures::TryFutureExt;
use mysten_network::server::ServerBuilder;
use narwhal_network::metrics::MetricsMakeCallbackHandler;
use narwhal_network::metrics::{NetworkConnectionMetrics, NetworkMetrics};
use prometheus::Registry;
use std::option::Option::None;
use std::{sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_core::authority_aggregator::{AuthAggMetrics, AuthorityAggregator};
use sui_core::authority_server::ValidatorService;
use sui_core::safe_client::SafeClientMetrics;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_core::transaction_streamer::TransactionStreamer;
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_active::ActiveAuthority,
    authority_client::{
        make_network_authority_client_sets_from_genesis,
        make_network_authority_client_sets_from_system_state, NetworkAuthorityClient,
    },
};
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::streaming_api::TransactionStreamingApiImpl;
use sui_json_rpc::transaction_builder_api::FullNodeTransactionBuilderApi;
use sui_network::api::ValidatorServer;
use sui_network::default_mysten_network_config;
use sui_network::discovery;
use sui_storage::{
    event_store::{EventStoreType, SqlEventStore},
    node_sync_store::NodeSyncStore,
    IndexStore,
};
use sui_types::messages::{CertifiedTransaction, CertifiedTransactionEffects};
use tokio::sync::mpsc::channel;
use tower::ServiceBuilder;
use tracing::{info, warn};
use typed_store::DBMetrics;

use crate::metrics::GrpcMetrics;
use sui_core::authority_client::NetworkAuthorityClientMetrics;
use sui_core::epoch::committee_store::CommitteeStore;
use sui_json_rpc::event_api::EventReadApiImpl;
use sui_json_rpc::event_api::EventStreamingApiImpl;
use sui_json_rpc::http_server::HttpServerHandle;
use sui_json_rpc::read_api::FullNodeApi;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::transaction_execution_api::FullNodeTransactionExecutionApi;
use sui_json_rpc::ws_server::WsServerHandle;
use sui_json_rpc::JsonRpcServerBuilder;
use sui_metrics::spawn_monitored_task;
use sui_types::crypto::KeypairTraits;

pub mod admin;
pub mod metrics;

mod handle;
pub use handle::SuiNodeHandle;
use narwhal_types::TransactionsClient;
use sui_core::checkpoints::{
    CheckpointMetrics, CheckpointService, LogCheckpointOutput, SubmitCheckpointToConsensus,
};

pub struct SuiNode {
    grpc_server: tokio::task::JoinHandle<Result<()>>,
    _json_rpc_service: Option<HttpServerHandle>,
    _ws_subscription_service: Option<WsServerHandle>,
    _batch_subsystem_handle: tokio::task::JoinHandle<()>,
    _post_processing_subsystem_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    _gossip_handle: Option<tokio::task::JoinHandle<()>>,
    _execute_driver_handle: tokio::task::JoinHandle<()>,
    state: Arc<AuthorityState>,
    active: Arc<ActiveAuthority<NetworkAuthorityClient>>,
    transaction_orchestrator: Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    _prometheus_registry: Registry,

    _p2p_network: anemo::Network,
    _discovery: discovery::Handle,

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
        sui_metrics::init_metrics(&prometheus_registry);

        let genesis = config.genesis()?;

        let secret = Arc::pin(config.protocol_key_pair().copy());
        let committee = genesis.committee()?;
        let store = Arc::new(AuthorityStore::open(&config.db_path().join("store"), None)?);
        let committee_store = Arc::new(CommitteeStore::new(
            config.db_path().join("epochs"),
            &committee,
            None,
        ));

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

        let (tx_reconfigure_consensus, rx_reconfigure_consensus) = channel(100);

        let transaction_streamer = config
            .websocket_address
            .map(|_| Arc::new(TransactionStreamer::new()));

        let node_sync_store = Arc::new(NodeSyncStore::open_tables_read_write(
            config.db_path().join("node_sync_db"),
            None,
            None,
        ));

        let consensus_client = config.consensus_config().map(|config| {
            let consensus_address = config.address().to_owned();
            TransactionsClient::new(
                mysten_network::client::connect_lazy(&consensus_address)
                    .expect("Failed to connect to consensus"),
            )
        });

        let checkpoint_output = if let Some(ref consensus_client) = consensus_client {
            Box::new(SubmitCheckpointToConsensus {
                sender: consensus_client.clone(),
                signer: secret.clone(),
                authority: config.protocol_public_key(),
            })
        } else {
            // todo - we should refactor code a bit and simply don't start checkpoint builder on full node
            LogCheckpointOutput::boxed()
        };

        let checkpoint_service = CheckpointService::spawn(
            &config.db_path().join("checkpoints"),
            Box::new(store.clone()),
            checkpoint_output,
            LogCheckpointOutput::boxed_certified(),
            committee.clone(),
            CheckpointMetrics::new(&prometheus_registry),
        );

        let state = Arc::new(
            AuthorityState::new(
                config.protocol_public_key(),
                secret,
                store,
                node_sync_store,
                committee_store.clone(),
                index_store.clone(),
                event_store,
                transaction_streamer,
                genesis,
                &prometheus_registry,
                tx_reconfigure_consensus,
                checkpoint_service,
            )
            .await,
        );
        let net_config = default_mysten_network_config();

        let sui_system_state = state.get_sui_system_state_object().await?;

        let network_metrics = Arc::new(NetworkAuthorityClientMetrics::new(&prometheus_registry));

        let authority_clients = if config.enable_reconfig && sui_system_state.epoch > 0 {
            make_network_authority_client_sets_from_system_state(
                &sui_system_state,
                &net_config,
                network_metrics.clone(),
            )
        } else {
            make_network_authority_client_sets_from_genesis(
                genesis,
                &net_config,
                network_metrics.clone(),
            )
        }?;
        let net = AuthorityAggregator::new(
            state.clone_committee(),
            committee_store,
            authority_clients,
            AuthAggMetrics::new(&prometheus_registry),
            Arc::new(SafeClientMetrics::new(&prometheus_registry)),
            network_metrics.clone(),
        );

        let active_authority = Arc::new(ActiveAuthority::new(
            state.clone(),
            net.clone(),
            &prometheus_registry,
            network_metrics.clone(),
        )?);

        let arc_net = active_authority.agg_aggregator();

        let transaction_orchestrator = if is_full_node {
            Some(Arc::new(TransactiondOrchestrator::new(
                arc_net,
                state.clone(),
                active_authority.clone().node_sync_handle(),
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
        let execute_driver_handle = active_authority.clone().spawn_execute_process().await;

        let registry = prometheus_registry.clone();
        let validator_service = if let Some(consensus_client) = consensus_client {
            Some(
                ValidatorService::new(
                    Box::new(consensus_client),
                    config,
                    state.clone(),
                    registry,
                    rx_reconfigure_consensus,
                )
                .await?,
            )
        } else {
            None
        };

        let grpc_server = {
            let mut server_conf = mysten_network::config::Config::new();
            server_conf.global_concurrency_limit = config.grpc_concurrency_limit;
            server_conf.load_shed = config.grpc_load_shed;
            let mut server_builder =
                ServerBuilder::from_config(&server_conf, GrpcMetrics::new(&prometheus_registry));

            if let Some(validator_service) = validator_service {
                server_builder =
                    server_builder.add_service(ValidatorServer::new(validator_service));
            }

            let server = server_builder
                .bind(config.network_address())
                .await
                .map_err(|err| anyhow!(err.to_string()))?;
            let local_addr = server.local_addr();
            info!("Listening to traffic on {local_addr}");
            spawn_monitored_task!(server.serve().map_err(Into::into))
        };

        let (discovery, discovery_server) = discovery::Builder::new()
            .config(config.p2p_config.clone())
            .build();

        let p2p_network = {
            let routes = anemo::Router::new().add_rpc_service(discovery_server);

            let inbound_network_metrics =
                NetworkMetrics::new("sui", "inbound", &prometheus_registry);
            let outbound_network_metrics =
                NetworkMetrics::new("sui", "outbound", &prometheus_registry);
            let network_connection_metrics =
                NetworkConnectionMetrics::new("sui", &prometheus_registry);

            let service = ServiceBuilder::new()
                .layer(TraceLayer::new_for_server_errors())
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(inbound_network_metrics),
                )))
                .service(routes);

            let outbound_layer = ServiceBuilder::new()
                .layer(TraceLayer::new_for_client_and_server_errors())
                .layer(CallbackLayer::new(MetricsMakeCallbackHandler::new(
                    Arc::new(outbound_network_metrics),
                )))
                .into_inner();

            let network = anemo::Network::bind(config.p2p_config.listen_address)
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
                );

            network
        };

        let discovery_handle = discovery.start(p2p_network.clone());

        let (json_rpc_service, ws_subscription_service) = build_http_servers(
            state.clone(),
            &transaction_orchestrator.clone(),
            config,
            &prometheus_registry,
        )
        .await?;

        let node = Self {
            grpc_server,
            _json_rpc_service: json_rpc_service,
            _ws_subscription_service: ws_subscription_service,
            _gossip_handle: gossip_handle,
            _execute_driver_handle: execute_driver_handle,
            _batch_subsystem_handle: batch_subsystem_handle,
            _post_processing_subsystem_handle: post_processing_subsystem_handle,
            state,
            active: active_authority,
            transaction_orchestrator,
            _prometheus_registry: prometheus_registry,
            _p2p_network: p2p_network,
            _discovery: discovery_handle,

            #[cfg(msim)]
            sim_node: sui_simulator::runtime::NodeHandle::current(),
        };

        info!("SuiNode started!");

        Ok(node)
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

    //TODO watch/wait on all the components
    pub async fn wait(self) -> Result<()> {
        self.grpc_server.await??;

        Ok(())
    }
}

pub async fn build_http_servers(
    state: Arc<AuthorityState>,
    transaction_orchestrator: &Option<Arc<TransactiondOrchestrator<NetworkAuthorityClient>>>,
    config: &NodeConfig,
    prometheus_registry: &Registry,
) -> Result<(Option<HttpServerHandle>, Option<WsServerHandle>)> {
    // Validators do not expose these APIs
    if config.consensus_config().is_some() {
        return Ok((None, None));
    }

    if cfg!(msim) {
        // jsonrpsee uses difficult-to-support features such as TcpSocket::from_raw_fd(), so we
        // can't yet run it in the simulator.
        warn!("disabling http servers in simulator");
        return Ok((None, None));
    }

    let mut server =
        JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), false, prometheus_registry)?;

    server.register_module(ReadApi::new(state.clone()))?;
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

    let rpc_server_handle = server
        .start(config.json_rpc_address)
        .await?
        .into_http_server_handle()
        .expect("Expect a http server handle");

    let ws_server_handle = match config.websocket_address {
        Some(ws_addr) => {
            let mut server =
                JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), true, prometheus_registry)?;
            if let Some(tx_streamer) = state.transaction_streamer.clone() {
                server.register_module(TransactionStreamingApiImpl::new(
                    state.clone(),
                    tx_streamer,
                ))?;
            } else {
                bail!("Expect State to have Some TransactionStreamer when websocket_address is present in node config");
            }
            if let Some(event_handler) = state.event_handler.clone() {
                server.register_module(EventStreamingApiImpl::new(state.clone(), event_handler))?;
            }
            Some(
                server
                    .start(ws_addr)
                    .await?
                    .into_ws_server_handle()
                    .expect("Expect a websocket server handle"),
            )
        }
        None => None,
    };
    Ok((Some(rpc_server_handle), ws_server_handle))
}
