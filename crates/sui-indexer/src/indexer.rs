// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;

use anyhow::Result;
use prometheus::Registry;
use tracing::info;

use mysten_metrics::spawn_monitored_task;

use crate::build_json_rpc_server;
use crate::errors::IndexerError;
use crate::framework::fetcher::CheckpointFetcher;
use crate::handlers::checkpoint_handler::new_handlers;
use crate::indexer_reader::IndexerReader;
use crate::metrics::IndexerMetrics;
use crate::processors::objects_snapshot_processor::{ObjectsSnapshotProcessor, SnapshotLagConfig};
use crate::processors::processor_orchestrator::ProcessorOrchestrator;
use crate::store::{IndexerStore, PgIndexerAnalyticalStore};
use crate::IndexerConfig;

const DOWNLOAD_QUEUE_SIZE: usize = 1000;

pub struct Indexer;

impl Indexer {
    pub async fn start_writer<S: IndexerStore + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let snapshot_config = SnapshotLagConfig::default();
        Indexer::start_writer_with_config(config, store, metrics, snapshot_config).await
    }

    pub async fn start_writer_with_config<S: IndexerStore + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
        snapshot_config: SnapshotLagConfig,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui Indexer Writer (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );

        // None will be returned when checkpoints table is empty.
        let last_seq_from_db = store
            .get_latest_tx_checkpoint_sequence_number()
            .await
            .expect("Failed to get latest tx checkpoint sequence number from DB");
        let download_queue_size = env::var("DOWNLOAD_QUEUE_SIZE")
            .unwrap_or_else(|_| DOWNLOAD_QUEUE_SIZE.to_string())
            .parse::<usize>()
            .expect("Invalid DOWNLOAD_QUEUE_SIZE");
        let (downloaded_checkpoint_data_sender, downloaded_checkpoint_data_receiver) =
            mysten_metrics::metered_channel::channel(
                download_queue_size,
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
            metrics.clone(),
        );
        spawn_monitored_task!(fetcher.run());

        let objects_snapshot_processor = ObjectsSnapshotProcessor::new_with_config(
            store.clone(),
            metrics.clone(),
            snapshot_config,
        );
        spawn_monitored_task!(objects_snapshot_processor.start());

        let checkpoint_handler = new_handlers(store, metrics).await?;
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
            "Sui Indexer Reader (version {:?}) started...",
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
            "Sui Indexer Analytical Worker (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );
        let mut processor_orchestrator = ProcessorOrchestrator::new(store, metrics);
        processor_orchestrator.run_forever().await;
        Ok(())
    }
}
