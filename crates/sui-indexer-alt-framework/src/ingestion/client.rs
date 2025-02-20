// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ingestion::local_client::LocalIngestionClient;
use crate::ingestion::remote_client::RemoteIngestionClient;
use crate::ingestion::Error as IngestionError;
use crate::ingestion::Result as IngestionResult;
use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use backoff::backoff::Constant;
use backoff::Error as BE;
use backoff::ExponentialBackoff;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio_util::bytes::Bytes;
use tokio_util::sync::CancellationToken;
use tracing::debug;
use url::Url;

/// Wait at most this long between retries for transient errors.
const MAX_TRANSIENT_RETRY_INTERVAL: Duration = Duration::from_secs(60);

#[async_trait::async_trait]
pub(crate) trait IngestionClientTrait: Send + Sync {
    async fn fetch(&self, checkpoint: u64) -> FetchResult;
}

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("Checkpoint not found")]
    NotFound,
    #[error("Failed to fetch checkpoint due to permanent error: {0}")]
    Permanent(#[from] anyhow::Error),
    #[error("Failed to fetch checkpoint due to {reason}: {error}")]
    Transient {
        reason: &'static str,
        #[source]
        error: anyhow::Error,
    },
}

pub type FetchResult = Result<Bytes, FetchError>;

#[derive(Clone)]
pub struct IngestionClient {
    client: Arc<dyn IngestionClientTrait>,
    /// Wrap the metrics in an `Arc` to keep copies of the client cheap.
    metrics: Arc<IndexerMetrics>,
    checkpoint_lag_reporter: Arc<CheckpointLagMetricReporter>,
}

impl IngestionClient {
    pub(crate) fn new_remote(url: Url, metrics: Arc<IndexerMetrics>) -> IngestionResult<Self> {
        let client = Arc::new(RemoteIngestionClient::new(url)?);
        Ok(Self::new_impl(client, metrics))
    }

    pub(crate) fn new_local(path: PathBuf, metrics: Arc<IndexerMetrics>) -> Self {
        let client = Arc::new(LocalIngestionClient::new(path));
        Self::new_impl(client, metrics)
    }

    fn new_impl(client: Arc<dyn IngestionClientTrait>, metrics: Arc<IndexerMetrics>) -> Self {
        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new(
            metrics.ingested_checkpoint_timestamp_lag.clone(),
            metrics.latest_ingested_checkpoint_timestamp_lag_ms.clone(),
            metrics.latest_ingested_checkpoint.clone(),
        );
        IngestionClient {
            client,
            metrics,
            checkpoint_lag_reporter,
        }
    }

    /// Fetch checkpoint data by sequence number.
    ///
    /// This function behaves like `IngestionClient::fetch`, but will repeatedly retry the fetch if
    /// the checkpoint is not found, on a constant back-off. The time between fetches is controlled
    /// by the `retry_interval` parameter.
    pub async fn wait_for(
        &self,
        checkpoint: u64,
        retry_interval: Duration,
        cancel: &CancellationToken,
    ) -> IngestionResult<Arc<CheckpointData>> {
        let backoff = Constant::new(retry_interval);
        let fetch = || async move {
            use backoff::Error as BE;
            if cancel.is_cancelled() {
                return Err(BE::permanent(IngestionError::Cancelled));
            }

            self.fetch(checkpoint, cancel).await.map_err(|e| match e {
                IngestionError::NotFound(checkpoint) => {
                    debug!(checkpoint, "Checkpoint not found, retrying...");
                    self.metrics.total_ingested_not_found_retries.inc();
                    BE::transient(e)
                }
                e => BE::permanent(e),
            })
        };

        backoff::future::retry(backoff, fetch).await
    }

    /// Fetch checkpoint data by sequence number.
    ///
    /// Repeatedly retries transient errors with an exponential backoff (up to
    /// [MAX_TRANSIENT_RETRY_INTERVAL]). Transient errors are either defined by the client
    /// implementation that returns a [FetchError::Transient] error variant, or within this
    /// function if we fail to deserialize the result as [CheckpointData].
    ///
    /// The function will immediately return on:
    ///
    /// - Non-transient errors determined by the client implementation, this includes both the
    ///   [FetchError::NotFound] and [FetchError::Permanent] variants.
    ///
    /// - Cancellation of the supplied `cancel` token.
    pub(crate) async fn fetch(
        &self,
        checkpoint: u64,
        cancel: &CancellationToken,
    ) -> IngestionResult<Arc<CheckpointData>> {
        let client = self.client.clone();
        let request = move || {
            let client = client.clone();
            async move {
                if cancel.is_cancelled() {
                    return Err(BE::permanent(IngestionError::Cancelled));
                }

                let bytes = client.fetch(checkpoint).await.map_err(|err| match err {
                    FetchError::NotFound => BE::permanent(IngestionError::NotFound(checkpoint)),
                    FetchError::Permanent(error) => {
                        BE::permanent(IngestionError::FetchError(checkpoint, error))
                    }
                    FetchError::Transient { reason, error } => self.metrics.inc_retry(
                        checkpoint,
                        reason,
                        IngestionError::FetchError(checkpoint, error),
                    ),
                })?;

                self.metrics.total_ingested_bytes.inc_by(bytes.len() as u64);
                let data: CheckpointData = Blob::from_bytes(&bytes).map_err(|e| {
                    self.metrics.inc_retry(
                        checkpoint,
                        "deserialization",
                        IngestionError::DeserializationError(checkpoint, e),
                    )
                })?;

                Ok(data)
            }
        };

        // Keep backing off until we are waiting for the max interval, but don't give up.
        let backoff = ExponentialBackoff {
            max_interval: MAX_TRANSIENT_RETRY_INTERVAL,
            max_elapsed_time: None,
            ..Default::default()
        };

        let guard = self.metrics.ingested_checkpoint_latency.start_timer();
        let data = backoff::future::retry(backoff, request).await?;
        let elapsed = guard.stop_and_record();

        debug!(
            checkpoint,
            elapsed_ms = elapsed * 1000.0,
            "Fetched checkpoint"
        );

        self.checkpoint_lag_reporter
            .report_lag(checkpoint, data.checkpoint_summary.timestamp_ms);

        self.metrics.total_ingested_checkpoints.inc();

        self.metrics
            .total_ingested_transactions
            .inc_by(data.transactions.len() as u64);

        self.metrics.total_ingested_events.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.events.as_ref().map_or(0, |evs| evs.data.len()) as u64)
                .sum(),
        );

        self.metrics.total_ingested_inputs.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.input_objects.len() as u64)
                .sum(),
        );

        self.metrics.total_ingested_outputs.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.output_objects.len() as u64)
                .sum(),
        );

        Ok(Arc::new(data))
    }
}
