// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use sui_rpc_api::ServerVersion;
use sui_types::digests::ChainIdentifier;
use sui_types::message_envelope::Message;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};
use tracing::error;

mod v2;

#[derive(Clone)]
pub struct KvRpcServer {
    chain_id: ChainIdentifier,
    client: BigTableClient,
    server_version: Option<ServerVersion>,
    checkpoint_bucket: Option<String>,
    cache: Arc<RwLock<Option<GetServiceInfoResponse>>>,
}

impl KvRpcServer {
    pub async fn new(
        instance_id: String,
        app_profile_id: Option<String>,
        checkpoint_bucket: Option<String>,
        server_version: Option<ServerVersion>,
        registry: &Registry,
    ) -> anyhow::Result<Self> {
        let mut client = BigTableClient::new_remote(
            instance_id,
            false,
            None,
            "sui-kv-rpc".to_string(),
            Some(registry),
            app_profile_id,
        )
        .await?;
        let genesis = client
            .get_checkpoints(&[0])
            .await?
            .pop()
            .expect("failed to fetch genesis checkpoint from the KV store");
        let chain_id = ChainIdentifier::from(genesis.summary.digest());
        let cache = Arc::new(RwLock::new(None));

        let server = Self {
            chain_id,
            client,
            server_version,
            checkpoint_bucket,
            cache,
        };

        let server_clone = server.clone();
        tokio::spawn(async move {
            loop {
                match v2::get_service_info(
                    server_clone.client.clone(),
                    server_clone.chain_id,
                    server_clone.server_version.clone(),
                )
                .await
                {
                    Ok(info) => {
                        let mut cache = server_clone.cache.write().await;
                        *cache = Some(info);
                    }
                    Err(e) => error!("Failed to update service info cache: {:?}", e),
                }
                sleep(Duration::from_millis(10)).await;
            }
        });

        Ok(server)
    }
}
