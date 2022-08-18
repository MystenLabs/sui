// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use anyhow::Result;
use futures::TryFutureExt;
use parking_lot::Mutex;
use prometheus::Registry;
use std::option::Option::None;
use std::{sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_core::authority_active::checkpoint_driver::CheckpointMetrics;
use sui_core::authority_aggregator::{AuthAggMetrics, AuthorityAggregator};
use sui_core::authority_server::ValidatorService;
use sui_core::safe_client::SafeClientMetrics;
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_active::{gossip::GossipMetrics, ActiveAuthority},
    authority_client::{
        make_network_authority_client_sets_from_genesis,
        make_network_authority_client_sets_from_system_state, NetworkAuthorityClient,
    },
    checkpoints::CheckpointStore,
};
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_network::api::ValidatorServer;
use sui_quorum_driver::QuorumDriverMetrics;
use sui_quorum_driver::{QuorumDriver, QuorumDriverHandler};
use sui_storage::{
    event_store::{EventStoreType, SqlEventStore},
    follower_store::FollowerStore,
    node_sync_store::NodeSyncStore,
    IndexStore,
};
use sui_types::messages::{CertifiedTransaction, CertifiedTransactionEffects};
use tracing::info;

use sui_core::authority_client::NetworkAuthorityClientMetrics;
use sui_core::epoch::epoch_store::EpochStore;
use sui_json_rpc::event_api::EventReadApiImpl;
use sui_json_rpc::event_api::EventStreamingApiImpl;
use sui_json_rpc::http_server::HttpServerHandle;
use sui_json_rpc::quorum_driver_api::FullNodeQuorumDriverApi;
use sui_json_rpc::read_api::FullNodeApi;
use sui_json_rpc::read_api::ReadApi;
use sui_json_rpc::ws_server::WsServerHandle;
use sui_json_rpc::JsonRpcServerBuilder;
use sui_types::crypto::KeypairTraits;
use typed_store::traits::DBMapTableUtil;

pub mod admin;
pub mod metrics;

pub struct SuiNode {
    grpc_server: tokio::task::JoinHandle<Result<()>>,
    _json_rpc_service: Option<HttpServerHandle>,
    _ws_subscription_service: Option<WsServerHandle>,
    _batch_subsystem_handle: tokio::task::JoinHandle<Result<()>>,
    _post_processing_subsystem_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    _gossip_handle: Option<tokio::task::JoinHandle<()>>,
    _execute_driver_handle: Option<tokio::task::JoinHandle<()>>,
    _checkpoint_process_handle: Option<tokio::task::JoinHandle<()>>,
    state: Arc<AuthorityState>,
    active: Option<Arc<ActiveAuthority<NetworkAuthorityClient>>>,
    quorum_driver_handler: Option<QuorumDriverHandler<NetworkAuthorityClient>>,
}

impl SuiNode {
    pub async fn start(config: &NodeConfig) -> Result<SuiNode> {
        // TODO: maybe have a config enum that takes care of this for us.
        let is_validator = config.consensus_config().is_some();
        let is_full_node = !is_validator;

        //
        // Start metrics server
        //
        info!(
            "Starting Prometheus HTTP endpoint at {}",
            config.metrics_address
        );
        let prometheus_registry = metrics::start_prometheus_server(config.metrics_address);

        info!(node =? config.public_key(),
            "Initializing sui-node listening on {}", config.network_address
        );

        let genesis = config.genesis()?;

        let secret = Arc::pin(config.key_pair().copy());
        let committee = genesis.committee()?;
        let store = Arc::new(AuthorityStore::open(&config.db_path().join("store"), None));
        let epoch_store = Arc::new(EpochStore::new(
            config.db_path().join("epochs"),
            &committee,
            None,
        ));

        let checkpoint_store = Arc::new(Mutex::new(CheckpointStore::open(
            &config.db_path().join("checkpoints"),
            None,
            committee.epoch,
            config.public_key(),
            secret.clone(),
        )?));

        let index_store = if is_validator {
            None
        } else {
            Some(Arc::new(IndexStore::open_tables_read_write(
                config.db_path().join("indexes"),
                None,
                None,
            )))
        };

        let follower_store = Arc::new(FollowerStore::open_tables_read_write(
            config.db_path().join("follower_db"),
            None,
            None,
        ));

        let event_store = if config.enable_event_processing {
            let path = config.db_path().join("events.db");
            let db = SqlEventStore::new_from_file(&path).await?;
            db.initialize().await?;
            Some(Arc::new(EventStoreType::SqlEventStore(db)))
        } else {
            None
        };

        let state = Arc::new(
            AuthorityState::new(
                config.public_key(),
                secret,
                store,
                epoch_store.clone(),
                index_store.clone(),
                event_store,
                Some(checkpoint_store),
                genesis,
                &prometheus_registry,
            )
            .await,
        );

        let mut net_config = mysten_network::config::Config::new();
        net_config.connect_timeout = Some(Duration::from_secs(5));
        net_config.request_timeout = Some(Duration::from_secs(5));
        net_config.http2_keepalive_interval = Some(Duration::from_secs(5));

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
            epoch_store,
            authority_clients,
            AuthAggMetrics::new(&prometheus_registry),
            SafeClientMetrics::new(&prometheus_registry),
        );

        let quorum_driver_handler = if is_full_node {
            Some(QuorumDriverHandler::new(
                net.clone(),
                QuorumDriverMetrics::new(&prometheus_registry),
            ))
        } else {
            None
        };
        let should_start_follower = is_full_node || config.enable_gossip;

        let mut active = None;

        let (gossip_handle, execute_driver_handle, checkpoint_process_handle) =
            if should_start_follower {
                let pending_store = Arc::new(NodeSyncStore::open_tables_read_write(
                    config.db_path().join("node_sync_db"),
                    None,
                    None,
                ));

                let active_authority = Arc::new(ActiveAuthority::new(
                    state.clone(),
                    pending_store,
                    follower_store,
                    net,
                    GossipMetrics::new(&prometheus_registry),
                    network_metrics.clone(),
                )?);
                active = Some(Arc::clone(&active_authority));

                if is_validator {
                    // TODO: get degree from config file.
                    let degree = 4;
                    (
                        Some(active_authority.clone().spawn_gossip_process(degree).await),
                        Some(active_authority.clone().spawn_execute_process().await),
                        Some(
                            active_authority
                                .spawn_checkpoint_process(
                                    CheckpointMetrics::new(&prometheus_registry),
                                    config.enable_reconfig,
                                )
                                .await,
                        ),
                    )
                } else {
                    // TODO: enable checkpoint sync on fullnode
                    // let metrics = CheckpointMetrics::new(&prometheus_registry);
                    // active_authority.sync_to_latest_checkpoint(&metrics).await?;
                    (
                        Some(active_authority.spawn_node_sync_process().await),
                        None,
                        None,
                    )
                }
            } else {
                (None, None, None)
            };

        let batch_subsystem_handle = {
            // Start batch system so that this node can be followed
            let batch_state = state.clone();
            tokio::task::spawn(async move {
                batch_state
                    .run_batch_service(1000, Duration::from_secs(1))
                    .await
                    .map_err(Into::into)
            })
        };

        let post_processing_subsystem_handle =
            if index_store.is_some() || config.enable_event_processing {
                let indexing_state = state.clone();
                Some(tokio::task::spawn(async move {
                    indexing_state
                        .run_tx_post_processing_process()
                        .await
                        .map_err(Into::into)
                }))
            } else {
                None
            };

        let validator_service = if config.consensus_config().is_some() {
            Some(ValidatorService::new(config, state.clone(), &prometheus_registry).await?)
        } else {
            None
        };

        let grpc_server = {
            let mut server_conf = mysten_network::config::Config::new();
            server_conf.global_concurrency_limit = config.grpc_concurrency_limit;
            server_conf.load_shed = config.grpc_load_shed;
            let mut server_builder = server_conf.server_builder();

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
            tokio::spawn(server.serve().map_err(Into::into))
        };

        let (json_rpc_service, ws_subscription_service) = build_node_server(
            state.clone(),
            &quorum_driver_handler,
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
            _checkpoint_process_handle: checkpoint_process_handle,
            _batch_subsystem_handle: batch_subsystem_handle,
            _post_processing_subsystem_handle: post_processing_subsystem_handle,
            state,
            active,
            quorum_driver_handler,
        };

        info!("SuiNode started!");

        Ok(node)
    }

    pub fn state(&self) -> Arc<AuthorityState> {
        self.state.clone()
    }

    pub fn active(&self) -> Option<Arc<ActiveAuthority<NetworkAuthorityClient>>> {
        self.active.clone()
    }

    pub fn quorum_driver(&self) -> Option<Arc<QuorumDriver<NetworkAuthorityClient>>> {
        self.quorum_driver_handler
            .as_ref()
            .map(|qdh| qdh.clone_quorum_driver())
    }

    pub fn subscribe_to_quorum_driver_effects(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<(CertifiedTransaction, CertifiedTransactionEffects)>>
    {
        self.quorum_driver_handler
            .as_ref()
            .map(|qdh| qdh.subscribe())
            .ok_or_else(|| anyhow::anyhow!("Quorum Driver is not enabled in this node."))
    }

    //TODO watch/wait on all the components
    pub async fn wait(self) -> Result<()> {
        self.grpc_server.await??;

        Ok(())
    }
}

pub async fn build_node_server(
    state: Arc<AuthorityState>,
    quorum_driver_handler: &Option<QuorumDriverHandler<NetworkAuthorityClient>>,
    config: &NodeConfig,
    prometheus_registry: &Registry,
) -> Result<(Option<HttpServerHandle>, Option<WsServerHandle>)> {
    // Validators do not expose these APIs
    if config.consensus_config().is_some() {
        return Ok((None, None));
    }
    let mut server = JsonRpcServerBuilder::new(false, prometheus_registry)?;

    server.register_module(ReadApi::new(state.clone()))?;
    server.register_module(FullNodeApi::new(state.clone()))?;
    server.register_module(BcsApiImpl::new(state.clone()))?;

    if let Some(quorum_driver_handler_) = quorum_driver_handler {
        server.register_module(FullNodeQuorumDriverApi::new(
            quorum_driver_handler_.clone_quorum_driver(),
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

    // TODO: we will change the conditions soon when we introduce txn subs
    let ws_server_handle = match (config.websocket_address, state.event_handler.clone()) {
        (Some(ws_addr), Some(event_handler)) => {
            let mut server = JsonRpcServerBuilder::new(true, prometheus_registry)?;
            server.register_module(EventStreamingApiImpl::new(state.clone(), event_handler))?;
            Some(
                server
                    .start(ws_addr)
                    .await?
                    .into_ws_server_handle()
                    .expect("Expect a websocket server handle"),
            )
        }
        _ => None,
    };
    Ok((Some(rpc_server_handle), ws_server_handle))
}
