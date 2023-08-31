// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;

use crate::IndexerConfig;
use anyhow::Result;

use crate::metrics::IndexerMetrics;
use prometheus::Registry;

use tracing::info;

use crate::errors::IndexerError;
use mysten_metrics::spawn_monitored_task;

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

        if config.rpc_server_worker {
            unimplemented!("not supported in v2 yet");
        }

        // It's a writer
        info!("Starting indexer with only fullnode sync");

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
