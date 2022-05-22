// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use sui_config::NodeConfig;
use sui_core::{
    authority::{AuthorityState, AuthorityStore},
    authority_active::{gossip::gossip_process, ActiveAuthority},
    authority_client::NetworkAuthorityClient,
};
use sui_gateway::api::{RpcGatewayOpenRpc, RpcGatewayServer};
use sui_gateway::json_rpc::JsonRpcServerBuilder;
use sui_gateway::read_api::ReadApi;
use sui_storage::IndexStore;
use tracing::info;

// TODO extract the important bits from AuthorityServer and FullNode so that we can have a single
// unified node. See https://github.com/MystenLabs/sui/issues/2068 for more info.
pub struct SuiNode;

impl SuiNode {
    pub async fn start(config: &NodeConfig) -> Result<()> {
        if config.consensus_config().is_some() {
            // Validator
            let server = sui_core::make::make_server(config).await?.spawn().await?;

            info!(node =? config.public_key(),
                "Initializing sui-node listening on {}", config.network_address
            );

            server.join().await?;
        } else {
            // Fullnode
            let fullnode = FullNode::start(config).await?;

            let mut server = JsonRpcServerBuilder::new()?;
            server.register_open_rpc(RpcGatewayOpenRpc::open_rpc())?;
            server.register_methods(
                ReadApi {
                    state: fullnode.state,
                }
                .into_rpc(),
            )?;

            let server_handle = server.start(config.json_rpc_address).await?;

            server_handle.await;
        }

        Ok(())
    }
}

//TODO remove the separate FullNode type and merge with the SuiNode type above
pub struct FullNode {
    pub state: Arc<AuthorityState>,
}

impl FullNode {
    pub async fn start(config: &NodeConfig) -> anyhow::Result<Self> {
        // TODO use ReplicaStore, or (more likely) get rid of ReplicaStore and
        // use run-time configuration to determine how much state AuthorityStore
        // keeps
        let store = Arc::new(AuthorityStore::open(config.db_path(), None));

        let index_path = config.db_path().join("indexes");
        let indexes = Arc::new(IndexStore::open(index_path, None));

        let state = Arc::new(
            AuthorityState::new(
                config.committee_config().committee(),
                config.public_key(),
                Arc::pin(config.key_pair().copy()),
                store,
                Some(indexes),
                None,
                config.genesis(),
            )
            .await,
        );

        let mut net_config = mysten_network::config::Config::new();
        net_config.connect_timeout = Some(Duration::from_secs(5));
        net_config.request_timeout = Some(Duration::from_secs(5));

        let mut authority_clients = BTreeMap::new();
        for validator in config.committee_config().validator_set() {
            let channel = net_config
                .connect_lazy(validator.network_address())
                .unwrap();
            let client = NetworkAuthorityClient::new(channel);
            authority_clients.insert(validator.public_key(), client);
        }

        let active_authority = ActiveAuthority::new(state.clone(), authority_clients)?;

        // Start following validators
        tokio::task::spawn(async move {
            gossip_process(
                &active_authority,
                // listen to all authorities (note that gossip_process caps this to total minus 1.)
                active_authority.state.committee.voting_rights.len(),
            )
            .await;
        });

        // Start batch system so the full node can be followed - currently only the
        // tests use this, in order to wait until the full node has seen a tx.
        // However, there's no reason full nodes won't want to follow other full nodes
        // eventually.
        let batch_state = state.clone();
        tokio::task::spawn(async move {
            batch_state
                .run_batch_service(1000, Duration::from_secs(1))
                .await
        });

        info!("Started full node ");

        Ok(Self { state })
    }
}
