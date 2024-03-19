// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::metrics::IndexerMetrics;

use super::fetcher::CheckpointFetcher;
use super::Handler;

pub struct IndexerBuilder {
    rest_url: Option<String>,
    handlers: Vec<Box<dyn Handler>>,
    last_downloaded_checkpoint: Option<CheckpointSequenceNumber>,
    checkpoint_buffer_size: usize,
    metrics: IndexerMetrics,
}

impl IndexerBuilder {
    const DEFAULT_CHECKPOINT_BUFFER_SIZE: usize = 1000;

    #[allow(clippy::new_without_default)]
    pub fn new(metrics: IndexerMetrics) -> Self {
        Self {
            rest_url: None,
            handlers: Vec::new(),
            last_downloaded_checkpoint: None,
            checkpoint_buffer_size: Self::DEFAULT_CHECKPOINT_BUFFER_SIZE,
            metrics,
        }
    }

    pub fn rest_url<T: Into<String>>(mut self, rest_url: T) -> Self {
        self.rest_url = Some(rest_url.into());
        self
    }

    pub fn handler<T: Handler + 'static>(mut self, handler: T) -> Self {
        self.handlers.push(Box::new(handler));
        self
    }

    pub fn last_downloaded_checkpoint(
        mut self,
        last_downloaded_checkpoint: Option<CheckpointSequenceNumber>,
    ) -> Self {
        self.last_downloaded_checkpoint = last_downloaded_checkpoint;
        self
    }

    pub fn checkpoint_buffer_size(mut self, checkpoint_buffer_size: usize) -> Self {
        self.checkpoint_buffer_size = checkpoint_buffer_size;
        self
    }

    pub async fn run(self) {
        let (downloaded_checkpoint_data_sender, downloaded_checkpoint_data_receiver) =
            mysten_metrics::metered_channel::channel(
                self.checkpoint_buffer_size,
                &mysten_metrics::get_metrics()
                    .unwrap()
                    .channels
                    .with_label_values(&["checkpoint_tx_downloading"]),
            );

        // experimental rest api route is found at `/rest` on the same interface as the jsonrpc
        // service
        let rest_api_url = format!("{}/rest", self.rest_url.unwrap());
        let fetcher = CheckpointFetcher::new(
            sui_rest_api::Client::new(rest_api_url),
            self.last_downloaded_checkpoint,
            downloaded_checkpoint_data_sender,
            self.metrics.clone(),
        );
        mysten_metrics::spawn_monitored_task!(fetcher.run());

        assert!(!self.handlers.is_empty());

        super::runner::run(
            mysten_metrics::metered_channel::ReceiverStream::new(
                downloaded_checkpoint_data_receiver,
            ),
            self.handlers,
        )
        .await;
    }
}
