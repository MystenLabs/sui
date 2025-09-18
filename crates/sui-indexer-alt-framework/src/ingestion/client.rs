// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use backoff::backoff::Constant;
use backoff::Error as BE;
use backoff::ExponentialBackoff;
use sui_rpc_api::client::AuthInterceptor;
use sui_rpc_api::Client;
use sui_storage::blob::Blob;
use tokio_util::bytes::Bytes;
use tracing::{debug, warn};
use url::Url;

use crate::ingestion::local_client::LocalIngestionClient;
use crate::ingestion::remote_client::RemoteIngestionClient;
use crate::ingestion::Error as IngestionError;
use crate::ingestion::Result as IngestionResult;
use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use crate::task::with_slow_future_monitor;
use crate::types::full_checkpoint_content::CheckpointData;

/// Wait at most this long between retries for transient errors.
const MAX_TRANSIENT_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// Threshold for logging warnings about slow HTTP operations during checkpoint fetching.
///
/// Operations that take longer than this duration will trigger a warning log, but will
/// continue executing without being canceled. This helps identify network issues or
/// slow remote stores without interrupting the ingestion process.
const SLOW_OPERATION_WARNING_THRESHOLD: Duration = Duration::from_secs(60);

#[async_trait::async_trait]
pub(crate) trait IngestionClientTrait: Send + Sync {
    async fn fetch(&self, checkpoint: u64) -> FetchResult;
}

#[derive(thiserror::Error, Debug)]
pub enum FetchError {
    #[error("Checkpoint not found")]
    NotFound,
    #[error("Failed to fetch checkpoint due to {reason}: {error}")]
    Transient {
        reason: &'static str,
        #[source]
        error: anyhow::Error,
    },
}

pub type FetchResult = Result<FetchData, FetchError>;

#[derive(Clone)]
pub enum FetchData {
    Raw(Bytes),
    CheckpointData(CheckpointData),
}

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

    pub(crate) fn new_rpc(
        url: Url,
        username: Option<String>,
        password: Option<String>,
        metrics: Arc<IndexerMetrics>,
    ) -> IngestionResult<Self> {
        let client = if let Some(username) = username {
            Client::new(url.to_string())?.with_auth(AuthInterceptor::basic(username, password))
        } else {
            Client::new(url.to_string())?
        };
        Ok(Self::new_impl(Arc::new(client), metrics))
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
    ) -> IngestionResult<Arc<CheckpointData>> {
        let backoff = Constant::new(retry_interval);
        let fetch = || async move {
            use backoff::Error as BE;
            self.fetch(checkpoint).await.map_err(|e| match e {
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
    /// The function will immediately return if the checkpoint is not found.
    pub(crate) async fn fetch(&self, checkpoint: u64) -> IngestionResult<Arc<CheckpointData>> {
        let client = self.client.clone();
        let request = move || {
            let client = client.clone();
            async move {
                let fetch_data = with_slow_future_monitor(
                    client.fetch(checkpoint),
                    SLOW_OPERATION_WARNING_THRESHOLD,
                    /* on_threshold_exceeded =*/
                    || {
                        warn!(
                            checkpoint,
                            threshold_ms = SLOW_OPERATION_WARNING_THRESHOLD.as_millis(),
                            "Slow checkpoint fetch operation detected"
                        );
                    },
                )
                .await
                .map_err(|err| match err {
                    FetchError::NotFound => BE::permanent(IngestionError::NotFound(checkpoint)),
                    FetchError::Transient { reason, error } => self.metrics.inc_retry(
                        checkpoint,
                        reason,
                        IngestionError::FetchError(checkpoint, error),
                    ),
                })?;

                Ok::<CheckpointData, backoff::Error<IngestionError>>(match fetch_data {
                    FetchData::Raw(bytes) => {
                        self.metrics.total_ingested_bytes.inc_by(bytes.len() as u64);
                        Blob::from_bytes(&bytes).map_err(|e| {
                            self.metrics.inc_retry(
                                checkpoint,
                                "deserialization",
                                IngestionError::DeserializationError(checkpoint, e),
                            )
                        })?
                    }
                    FetchData::CheckpointData(data) => {
                        // We are not recording size metric for Checkpoint data (from RPC client).
                        // TODO: Record the metric when we have a good way to get the size information
                        data
                    }
                })
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

#[cfg(test)]
mod tests {
    use dashmap::DashMap;
    use prometheus::Registry;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;
    use tokio_util::bytes::Bytes;

    use crate::ingestion::test_utils::test_checkpoint_data;

    use super::*;

    /// Mock implementation of IngestionClientTrait for testing
    #[derive(Default)]
    struct MockIngestionClient {
        checkpoints: DashMap<u64, FetchData>,
        transient_failures: DashMap<u64, usize>,
        not_found_failures: DashMap<u64, usize>,
    }

    #[async_trait::async_trait]
    impl IngestionClientTrait for MockIngestionClient {
        async fn fetch(&self, checkpoint: u64) -> FetchResult {
            // Check for not found failures
            if let Some(mut remaining) = self.not_found_failures.get_mut(&checkpoint) {
                if *remaining > 0 {
                    *remaining -= 1;
                    return Err(FetchError::NotFound);
                }
            }

            // Check for transient failures
            if let Some(mut remaining) = self.transient_failures.get_mut(&checkpoint) {
                if *remaining > 0 {
                    *remaining -= 1;
                    return Err(FetchError::Transient {
                        reason: "mock_transient_error",
                        error: anyhow::anyhow!("Mock transient error"),
                    });
                }
            }

            // Return the checkpoint data if it exists
            self.checkpoints
                .get(&checkpoint)
                .as_deref()
                .cloned()
                .ok_or(FetchError::NotFound)
        }
    }

    fn setup_test() -> (IngestionClient, Arc<MockIngestionClient>) {
        let registry = Registry::new_custom(Some("test".to_string()), None).unwrap();
        let metrics = IndexerMetrics::new(None, &registry);
        let mock_client = Arc::new(MockIngestionClient::default());
        let client = IngestionClient::new_impl(mock_client.clone(), metrics);
        (client, mock_client)
    }

    #[tokio::test]
    async fn test_fetch_raw_bytes_success() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint_data
        let bytes = Bytes::from(test_checkpoint_data(1));
        mock.checkpoints.insert(1, FetchData::Raw(bytes.clone()));

        // Fetch and verify
        let result = client.fetch(1).await.unwrap();
        assert_eq!(result.checkpoint_summary.sequence_number(), &1);
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_data_success() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint_data
        let bytes = test_checkpoint_data(1);
        let checkpoint_data: CheckpointData = Blob::from_bytes(&bytes).unwrap();
        mock.checkpoints
            .insert(1, FetchData::CheckpointData(checkpoint_data.clone()));

        // Fetch and verify
        let result = client.fetch(1).await.unwrap();
        assert_eq!(result.checkpoint_summary.sequence_number(), &1);
    }

    #[tokio::test]
    async fn test_fetch_not_found() {
        let (client, _) = setup_test();

        // Try to fetch non-existent checkpoint
        let result = client.fetch(1).await;
        assert!(matches!(result, Err(IngestionError::NotFound(1))));
    }

    #[tokio::test]
    async fn test_fetch_transient_error_with_retry() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint_data
        let bytes = test_checkpoint_data(1);
        let checkpoint_data: CheckpointData = Blob::from_bytes(&bytes).unwrap();

        // Add checkpoint to mock with 2 transient failures
        mock.checkpoints
            .insert(1, FetchData::CheckpointData(checkpoint_data.clone()));
        mock.transient_failures.insert(1, 2);

        // Fetch and verify it succeeds after retries
        let result = client.fetch(1).await.unwrap();
        assert_eq!(*result.checkpoint_summary.sequence_number(), 1);

        // Verify that exactly 2 retries were recorded
        let retries = client
            .metrics
            .total_ingested_transient_retries
            .with_label_values(&["mock_transient_error"])
            .get();
        assert_eq!(retries, 2);
    }

    #[tokio::test]
    async fn test_wait_for_checkpoint_with_retry() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint_data
        let bytes = test_checkpoint_data(1);
        let checkpoint_data: CheckpointData = Blob::from_bytes(&bytes).unwrap();

        // Add checkpoint to mock with 1 not_found failures
        mock.checkpoints
            .insert(1, FetchData::CheckpointData(checkpoint_data));
        mock.not_found_failures.insert(1, 1);

        // Wait for checkpoint with short retry interval
        let result = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.checkpoint_summary.sequence_number(), &1);

        // Verify that exactly 1 retry was recorded
        let retries = client.metrics.total_ingested_not_found_retries.get();
        assert_eq!(retries, 1);
    }

    #[tokio::test]
    async fn test_wait_for_checkpoint_instant() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint_data
        let bytes = test_checkpoint_data(1);
        let checkpoint_data: CheckpointData = Blob::from_bytes(&bytes).unwrap();

        // Add checkpoint to mock with no failures - data should be available immediately
        mock.checkpoints
            .insert(1, FetchData::CheckpointData(checkpoint_data));

        // Wait for checkpoint with short retry interval
        let result = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.checkpoint_summary.sequence_number(), &1);
    }

    #[tokio::test]
    async fn test_wait_for_permanent_deserialization_error() {
        let (client, mock) = setup_test();

        // Add invalid data that will cause a deserialization error
        mock.checkpoints
            .insert(1, FetchData::Raw(Bytes::from("invalid data")));

        // wait_for should keep retrying on deserialization errors and timeout
        timeout(
            Duration::from_secs(1),
            client.wait_for(1, Duration::from_millis(50)),
        )
        .await
        .unwrap_err();
    }
}
