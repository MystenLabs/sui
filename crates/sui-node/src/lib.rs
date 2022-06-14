// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::TryFutureExt;
use parking_lot::Mutex;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_core::authority_server::ValidatorService;
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_active::ActiveAuthority,
    authority_client::NetworkAuthorityClient,
    checkpoints::CheckpointStore,
};
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::JsonRpcServerBuilder;
use sui_network::api::ValidatorServer;
use sui_storage::{follower_store::FollowerStore, IndexStore};
use tracing::info;

use sui_json_rpc::read_api::FullNodeApi;
use sui_json_rpc::read_api::ReadApi;

pub struct SuiNode {
    grpc_server: tokio::task::JoinHandle<Result<()>>,
    _json_rpc_service: Option<jsonrpsee::http_server::HttpServerHandle>,
    _batch_subsystem_handle: tokio::task::JoinHandle<Result<()>>,
    _post_processing_subsystem_handle: Option<tokio::task::JoinHandle<Result<()>>>,
    _gossip_handle: Option<tokio::task::JoinHandle<()>>,
    state: Arc<AuthorityState>,
}

impl SuiNode {
    pub async fn start(config: &NodeConfig) -> Result<SuiNode> {
        info!(node =? config.public_key(),
            "Initializing sui-node listening on {}", config.network_address
        );

        let genesis = config.genesis()?;

        let secret = Arc::pin(config.key_pair().copy());
        let store = Arc::new(AuthorityStore::open(config.db_path().join("store"), None));
        let checkpoint_store = if config.consensus_config().is_some() {
            Some(Arc::new(Mutex::new(CheckpointStore::open(
                config.db_path().join("checkpoints"),
                None,
                config.public_key(),
                secret.clone(),
            )?)))
        } else {
            None
        };

        let index_store = if config.consensus_config().is_some() {
            None
        } else {
            Some(Arc::new(IndexStore::open(
                config.db_path().join("indexes"),
                None,
            )))
        };

        let follower_store = Arc::new(FollowerStore::open(config.db_path().join("follower_db"))?);

        let state = Arc::new(
            AuthorityState::new(
                genesis.committee(),
                config.public_key(),
                secret,
                store,
                index_store.clone(),
                checkpoint_store,
                genesis,
                config.enable_event_processing,
            )
            .await,
        );

        let gossip_handle = if config.enable_gossip {
            let mut net_config = mysten_network::config::Config::new();
            net_config.connect_timeout = Some(Duration::from_secs(5));
            net_config.request_timeout = Some(Duration::from_secs(5));
            net_config.http2_keepalive_interval = Some(Duration::from_secs(5));

            let mut authority_clients = BTreeMap::new();
            for validator in genesis.validator_set() {
                let channel = net_config
                    .connect_lazy(validator.network_address())
                    .unwrap();
                let client = NetworkAuthorityClient::new(channel);
                authority_clients.insert(validator.public_key(), client);
            }

            let active_authority =
                ActiveAuthority::new(state.clone(), follower_store, authority_clients)?;

            let degree = active_authority.state.committee.load().voting_rights.len();
            // Start following validators
            let handle = active_authority
                .spawn_gossip_process(
                    // listen to all authorities (note that gossip_process caps this to total minus 1.)
                    degree,
                )
                .await;
            Some(handle)
        } else {
            None
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
            Some(ValidatorService::new(config, state.clone()).await?)
        } else {
            None
        };

        let grpc_server = {
            let mut server_builder = mysten_network::config::Config::new().server_builder();

            if let Some(validator_service) = validator_service {
                server_builder =
                    server_builder.add_service(ValidatorServer::new(validator_service));
            }

            let server = server_builder.bind(config.network_address()).await?;
            let local_addr = server.local_addr();
            info!("Listening to traffic on {local_addr}");
            tokio::spawn(server.serve().map_err(Into::into))
        };

        let json_rpc_service = if config.consensus_config().is_some() {
            None
        } else {
            let mut server = JsonRpcServerBuilder::new()?;
            server.register_module(ReadApi::new(state.clone()))?;
            server.register_module(FullNodeApi::new(state.clone()))?;
            server.register_module(BcsApiImpl::new(state.clone()))?;

            let server_handle = server.start(config.json_rpc_address).await?;
            Some(server_handle)
        };

        let node = Self {
            grpc_server,
            _json_rpc_service: json_rpc_service,
            _gossip_handle: gossip_handle,
            _batch_subsystem_handle: batch_subsystem_handle,
            _post_processing_subsystem_handle: post_processing_subsystem_handle,
            state,
        };

        info!("SuiNode started!");

        Ok(node)
    }

    pub fn state(&self) -> Arc<AuthorityState> {
        self.state.clone()
    }

    //TODO watch/wait on all the components
    pub async fn wait(self) -> Result<()> {
        self.grpc_server.await??;

        Ok(())
    }
}
