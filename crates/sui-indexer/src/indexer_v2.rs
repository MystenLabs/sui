// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::apis::{
    CoinReadApiV2, ExtendedApiV2, GovernanceReadApiV2, IndexerApiV2, MoveUtilsApiV2, ReadApiV2,
    TransactionBuilderApiV2, WriteApi,
};
use crate::errors::IndexerError;
use crate::indexer_reader::IndexerReader;
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
use crate::processors_v2::objects_snapshot_processor::{
    ObjectsSnapshotProcessor, SnapshotLagConfig,
};
use crate::processors_v2::processor_orchestrator_v2::ProcessorOrchestratorV2;
use crate::store::{IndexerStoreV2, PgIndexerAnalyticalStore};

pub struct IndexerV2;

const DOWNLOAD_QUEUE_SIZE: usize = 1000;

impl IndexerV2 {
    pub async fn start_writer<S: IndexerStoreV2 + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let snapshot_config = SnapshotLagConfig::default();
        IndexerV2::start_writer_with_config(config, store, metrics, snapshot_config).await
    }

    pub async fn start_writer_with_config<S: IndexerStoreV2 + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
        snapshot_config: SnapshotLagConfig,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui indexerV2 Writer (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );

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

        let objects_snapshot_processor = ObjectsSnapshotProcessor::new_with_config(
            store.clone(),
            metrics.clone(),
            snapshot_config,
        );

        spawn_monitored_task!(objects_snapshot_processor.start());

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

    pub async fn start_reader(
        config: &IndexerConfig,
        registry: &Registry,
        db_url: String,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui indexerV2 Reader (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );
        let indexer_reader = IndexerReader::new(db_url)?;
        let handle = build_json_rpc_server(registry, indexer_reader, config, None)
            .await
            .expect("Json rpc server should not run into errors upon start.");
        tokio::spawn(async move { handle.stopped().await })
            .await
            .expect("Rpc server task failed");

        Ok(())
    }

    pub async fn start_analytical_worker(
        store: PgIndexerAnalyticalStore,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui indexerV2 Analytical Worker (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );
        let mut processor_orchestrator_v2 = ProcessorOrchestratorV2::new(store, metrics);
        processor_orchestrator_v2.run_forever().await;
        Ok(())
    }
}

pub async fn build_json_rpc_server(
    prometheus_registry: &Registry,
    reader: IndexerReader,
    config: &IndexerConfig,
    custom_runtime: Option<Handle>,
) -> Result<ServerHandle, IndexerError> {
    let mut builder = JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry);
    let http_client = crate::get_http_client(config.rpc_client_url.as_str())?;

    builder.register_module(WriteApi::new(http_client.clone()))?;
    builder.register_module(IndexerApiV2::new(reader.clone()))?;
    builder.register_module(TransactionBuilderApiV2::new(reader.clone()))?;
    builder.register_module(MoveUtilsApiV2::new(reader.clone()))?;
    builder.register_module(GovernanceReadApiV2::new(reader.clone()))?;
    builder.register_module(ReadApiV2::new(reader.clone()))?;
    builder.register_module(CoinReadApiV2::new(reader.clone()))?;
    builder.register_module(ExtendedApiV2::new(reader.clone()))?;

    let default_socket_addr: SocketAddr = SocketAddr::new(
        // unwrap() here is safe b/c the address is a static config.
        config.rpc_server_url.as_str().parse().unwrap(),
        config.rpc_server_port,
    );
    Ok(builder
        .start(default_socket_addr, custom_runtime, Some(ServerType::Http))
        .await?)
}
