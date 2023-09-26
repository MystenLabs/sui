// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::metrics::IndexerMetrics;
use crate::IndexerConfig;
use anyhow::Result;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use std::env;
use std::net::SocketAddr;
use sui_json_rpc::ServerType;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use tokio::runtime::Handle;
use tracing::info;

use crate::framework::fetcher::CheckpointFetcher;
use crate::handlers::checkpoint_handler_v2::new_handlers;
use crate::store::IndexerStoreV2;

pub struct IndexerV2;

const DOWNLOAD_QUEUE_SIZE: usize = 1000;

impl IndexerV2 {
    pub async fn start<S: IndexerStoreV2 + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        registry: &Registry,
        store: S,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui indexer of version {:?} started...",
            env!("CARGO_PKG_VERSION")
        );
        mysten_metrics::init_metrics(registry);

        // For testing purposes, for the time being we allow an indexer to be a
        // reader and writer at the same time
        let _handle = if config.rpc_server_worker {
            info!("Starting indexer reader");
            let handle = build_json_rpc_server(registry, store.clone(), config, None)
                .await
                .expect("Json rpc server should not run into errors upon start.");
            Some(tokio::spawn(async move { handle.stopped().await }))
        } else {
            None
        };

        if !config.fullnode_sync_worker {
            if let Some(handle) = _handle {
                handle.await.expect("Rpc server task failed");
            }
            return Ok(());
        }

        info!("Starting fullnode sync worker");
        // None will be returned when checkpoints table is empty.
        let last_seq_from_db = store
            .get_latest_tx_checkpoint_sequence_number()
            .await
            .expect("Failed to get latest tx checkpoint sequence number from DB");
        let (downloaded_checkpoint_data_sender, downloaded_checkpoint_data_receiver) =
            mysten_metrics::metered_channel::channel(
                DOWNLOAD_QUEUE_SIZE,
                &mysten_metrics::get_metrics()
                    .unwrap()
                    .channels
                    .with_label_values(&["checkpoint_tx_downloading"]),
            );

        let rest_api_url = format!("{}/rest", config.rpc_client_url);
        let rest_client = sui_rest_api::Client::new(&rest_api_url);
        let fetcher = CheckpointFetcher::new(
            rest_client.clone(),
            last_seq_from_db,
            downloaded_checkpoint_data_sender,
        );
        spawn_monitored_task!(fetcher.run());

        let checkpoint_handler = new_handlers(store, metrics, config).await?;

        crate::framework::runner::run(
            mysten_metrics::metered_channel::ReceiverStream::new(
                downloaded_checkpoint_data_receiver,
            ),
            vec![Box::new(checkpoint_handler)],
        )
        .await;

        Ok(())
    }
}

pub async fn build_json_rpc_server<S: IndexerStoreV2 + Sync + Send + 'static + Clone>(
    prometheus_registry: &Registry,
    _state: S,
    config: &IndexerConfig,
    custom_runtime: Option<Handle>,
) -> Result<ServerHandle, IndexerError> {
    let builder = JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry);

    // TODO: Register modules here
    // builder.register_module()...

    let default_socket_addr: SocketAddr = SocketAddr::new(
        // unwrap() here is safe b/c the address is a static config.
        config.rpc_server_url.as_str().parse().unwrap(),
        config.rpc_server_port,
    );
    Ok(builder
        .start(default_socket_addr, custom_runtime, Some(ServerType::Http))
        .await?)
}
