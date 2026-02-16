// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use backoff::Error as BE;
use backoff::ExponentialBackoff;
use backoff::backoff::Constant;
use bytes::Bytes;
use object_store::ClientOptions;
use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::azure::MicrosoftAzureBuilder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::http::HttpBuilder;
use object_store::local::LocalFileSystem;
use sui_futures::future::with_slow_future_monitor;
use sui_rpc::Client;
use sui_rpc::client::HeadersInterceptor;
use tracing::debug;
use tracing::error;
use tracing::warn;
use url::Url;

use crate::ingestion::Error as IngestionError;
use crate::ingestion::MAX_GRPC_MESSAGE_SIZE_BYTES;
use crate::ingestion::Result as IngestionResult;
use crate::ingestion::decode;
use crate::ingestion::store_client::StoreIngestionClient;
use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IngestionMetrics;
use crate::types::full_checkpoint_content::Checkpoint;

/// Wait at most this long between retries for transient errors.
const MAX_TRANSIENT_RETRY_INTERVAL: Duration = Duration::from_secs(60);

/// Threshold for logging warnings about slow HTTP operations during checkpoint fetching.
///
/// Operations that take longer than this duration will trigger a warning log, but will
/// continue executing without being canceled. This helps identify network issues or
/// slow remote stores without interrupting the ingestion process.
const SLOW_OPERATION_WARNING_THRESHOLD: Duration = Duration::from_secs(60);

#[async_trait]
pub(crate) trait IngestionClientTrait: Send + Sync {
    async fn fetch(&self, checkpoint: u64) -> FetchResult;
}

#[derive(clap::Args, Clone, Debug)]
#[group(required = true)]
pub struct IngestionClientArgs {
    /// Remote Store to fetch checkpoints from over HTTP.
    #[arg(long, group = "source")]
    pub remote_store_url: Option<Url>,

    /// Fetch checkpoints from AWS S3. Provide the bucket name or endpoint-and-bucket.
    /// (env: AWS_ENDPOINT, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION)
    #[arg(long, group = "source")]
    pub remote_store_s3: Option<String>,

    /// Fetch checkpoints from Google Cloud Storage. Provide the bucket name.
    /// (env: GOOGLE_SERVICE_ACCOUNT_PATH)
    #[arg(long, group = "source")]
    pub remote_store_gcs: Option<String>,

    /// Fetch checkpoints from Azure Blob Storage. Provide the container name.
    /// (env: AZURE_STORAGE_ACCOUNT_NAME, AZURE_STORAGE_ACCESS_KEY)
    #[arg(long, group = "source")]
    pub remote_store_azure: Option<String>,

    /// Path to the local ingestion directory.
    #[arg(long, group = "source")]
    pub local_ingestion_path: Option<PathBuf>,

    /// Sui fullnode gRPC url to fetch checkpoints from.
    #[arg(long, group = "source")]
    pub rpc_api_url: Option<Url>,

    /// Optional username for the gRPC service.
    #[arg(long, env)]
    pub rpc_username: Option<String>,

    /// Optional password for the gRPC service.
    #[arg(long, env)]
    pub rpc_password: Option<String>,

    /// How long to wait for a checkpoint file to be downloaded (milliseconds). Set to 0 to disable
    /// the timeout.
    #[arg(long, default_value_t = Self::default().checkpoint_timeout_ms)]
    pub checkpoint_timeout_ms: u64,

    /// How long to wait while establishing a connection to the checkpoint store (milliseconds).
    /// Set to 0 to disable the timeout.
    #[arg(long, default_value_t = Self::default().checkpoint_connection_timeout_ms)]
    pub checkpoint_connection_timeout_ms: u64,
}

impl Default for IngestionClientArgs {
    fn default() -> Self {
        Self {
            remote_store_url: None,
            remote_store_s3: None,
            remote_store_gcs: None,
            remote_store_azure: None,
            local_ingestion_path: None,
            rpc_api_url: None,
            rpc_username: None,
            rpc_password: None,
            checkpoint_timeout_ms: 120_000,
            checkpoint_connection_timeout_ms: 120_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IngestionMode {
    RemoteStore,
    FullNode,
}

impl IngestionClientArgs {
    pub(crate) fn ingestion_mode(&self) -> IngestionMode {
        if self.rpc_api_url.is_some() {
            IngestionMode::FullNode
        } else {
            IngestionMode::RemoteStore
        }
    }

    fn client_options(&self) -> ClientOptions {
        let mut options = ClientOptions::default();
        options = if self.checkpoint_timeout_ms == 0 {
            options.with_timeout_disabled()
        } else {
            let timeout = Duration::from_millis(self.checkpoint_timeout_ms);
            options.with_timeout(timeout)
        };
        options = if self.checkpoint_connection_timeout_ms == 0 {
            options.with_connect_timeout_disabled()
        } else {
            let timeout = Duration::from_millis(self.checkpoint_connection_timeout_ms);
            options.with_connect_timeout(timeout)
        };
        options
    }
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
    #[error("Permanent error in {reason}: {error}")]
    Permanent {
        reason: &'static str,
        #[source]
        error: anyhow::Error,
    },
}

pub type FetchResult = Result<FetchData, FetchError>;

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum FetchData {
    Raw(Bytes),
    Checkpoint(Checkpoint),
}

#[derive(Clone)]
pub struct IngestionClient {
    client: Arc<dyn IngestionClientTrait>,
    /// Wrap the metrics in an `Arc` to keep copies of the client cheap.
    metrics: Arc<IngestionMetrics>,
    checkpoint_lag_reporter: Arc<CheckpointLagMetricReporter>,
}

impl IngestionClient {
    /// Construct a new ingestion client. Its source is determined by `args`.
    pub fn new(args: IngestionClientArgs, metrics: Arc<IngestionMetrics>) -> IngestionResult<Self> {
        // TODO: Support stacking multiple ingestion clients for redundancy/failover.
        let retry = super::store_client::retry_config();
        let client = if let Some(url) = args.remote_store_url.as_ref() {
            let store = HttpBuilder::new()
                .with_url(url.to_string())
                .with_client_options(args.client_options().with_allow_http(true))
                .with_retry(retry)
                .build()
                .map(Arc::new)?;
            IngestionClient::with_store(store, metrics.clone())?
        } else if let Some(bucket) = args.remote_store_s3.as_ref() {
            let store = AmazonS3Builder::from_env()
                .with_client_options(args.client_options())
                .with_retry(retry)
                .with_imdsv1_fallback()
                .with_bucket_name(bucket)
                .build()
                .map(Arc::new)?;
            IngestionClient::with_store(store, metrics.clone())?
        } else if let Some(bucket) = args.remote_store_gcs.as_ref() {
            let store = GoogleCloudStorageBuilder::from_env()
                .with_client_options(args.client_options())
                .with_retry(retry)
                .with_bucket_name(bucket)
                .build()
                .map(Arc::new)?;
            IngestionClient::with_store(store, metrics.clone())?
        } else if let Some(container) = args.remote_store_azure.as_ref() {
            let store = MicrosoftAzureBuilder::from_env()
                .with_client_options(args.client_options())
                .with_retry(retry)
                .with_container_name(container)
                .build()
                .map(Arc::new)?;
            IngestionClient::with_store(store, metrics.clone())?
        } else if let Some(path) = args.local_ingestion_path.as_ref() {
            let store = LocalFileSystem::new_with_prefix(path).map(Arc::new)?;
            IngestionClient::with_store(store, metrics.clone())?
        } else if let Some(rpc_api_url) = args.rpc_api_url.as_ref() {
            IngestionClient::with_grpc(
                rpc_api_url.clone(),
                args.rpc_username,
                args.rpc_password,
                metrics.clone(),
            )?
        } else {
            panic!(
                "One of remote_store_url, remote_store_s3, remote_store_gcs, remote_store_azure, \
                local_ingestion_path or rpc_api_url must be provided"
            );
        };

        Ok(client)
    }

    /// An ingestion client that fetches checkpoints from a remote object store.
    pub fn with_store(
        store: Arc<dyn ObjectStore>,
        metrics: Arc<IngestionMetrics>,
    ) -> IngestionResult<Self> {
        let client = Arc::new(StoreIngestionClient::new(store));
        Ok(Self::new_impl(client, metrics))
    }

    /// An ingestion client that fetches checkpoints from a fullnode, over gRPC.
    pub fn with_grpc(
        url: Url,
        username: Option<String>,
        password: Option<String>,
        metrics: Arc<IngestionMetrics>,
    ) -> IngestionResult<Self> {
        let client = if let Some(username) = username {
            let mut headers = HeadersInterceptor::new();
            headers.basic_auth(username, password);
            Client::new(url.to_string())?
                .with_headers(headers)
                .with_max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE_BYTES)
        } else {
            Client::new(url.to_string())?
                .with_max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE_BYTES)
        };
        Ok(Self::new_impl(Arc::new(client), metrics))
    }

    pub(crate) fn new_impl(
        client: Arc<dyn IngestionClientTrait>,
        metrics: Arc<IngestionMetrics>,
    ) -> Self {
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
    ) -> IngestionResult<(Arc<Checkpoint>, usize)> {
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
    /// `MAX_TRANSIENT_RETRY_INTERVAL`). Transient errors are either defined by the client
    /// implementation that returns a [FetchError::Transient] error variant, or within this
    /// function if we fail to deserialize the result as [Checkpoint].
    ///
    /// The function will immediately return if the checkpoint is not found.
    pub async fn fetch(&self, checkpoint: u64) -> IngestionResult<(Arc<Checkpoint>, usize)> {
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
                    FetchError::Permanent { reason, error } => {
                        error!(checkpoint, reason, "Permanent fetch error: {error}");
                        self.metrics
                            .total_ingested_permanent_errors
                            .with_label_values(&[reason])
                            .inc();
                        BE::permanent(IngestionError::FetchError(checkpoint, error))
                    }
                })?;

                Ok::<(Checkpoint, usize), backoff::Error<IngestionError>>(match fetch_data {
                    FetchData::Raw(bytes) => {
                        let wire_size = bytes.len();
                        self.metrics.total_ingested_bytes.inc_by(wire_size as u64);

                        let data = decode::checkpoint(&bytes).map_err(|e| {
                            self.metrics.inc_retry(
                                checkpoint,
                                e.reason(),
                                IngestionError::DeserializationError(checkpoint, e.into()),
                            )
                        })?;
                        (data, wire_size)
                    }
                    FetchData::Checkpoint(data) => (data, 0),
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
        let (data, wire_size) = backoff::future::retry(backoff, request).await?;
        let elapsed = guard.stop_and_record();

        debug!(
            checkpoint,
            elapsed_ms = elapsed * 1000.0,
            "Fetched checkpoint"
        );

        self.checkpoint_lag_reporter
            .report_lag(checkpoint, data.summary.timestamp_ms);

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

        self.metrics
            .total_ingested_objects
            .inc_by(data.object_set.len() as u64);

        Ok((Arc::new(data), wire_size))
    }
}

#[cfg(test)]
mod tests {
    use dashmap::DashMap;
    use prometheus::Registry;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    use crate::ingestion::test_utils::test_checkpoint_data;

    use super::*;

    /// Mock implementation of IngestionClientTrait for testing
    #[derive(Default)]
    struct MockIngestionClient {
        checkpoints: DashMap<u64, FetchData>,
        transient_failures: DashMap<u64, usize>,
        not_found_failures: DashMap<u64, usize>,
        permanent_failures: DashMap<u64, usize>,
    }

    #[async_trait]
    impl IngestionClientTrait for MockIngestionClient {
        async fn fetch(&self, checkpoint: u64) -> FetchResult {
            // Check for not found failures
            if let Some(mut remaining) = self.not_found_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(FetchError::NotFound);
            }

            // Check for non-retryable failures
            if let Some(mut remaining) = self.permanent_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(FetchError::Permanent {
                    reason: "mock_permanent_error",
                    error: anyhow::anyhow!("Mock permanent error"),
                });
            }

            // Check for transient failures
            if let Some(mut remaining) = self.transient_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(FetchError::Transient {
                    reason: "mock_transient_error",
                    error: anyhow::anyhow!("Mock transient error"),
                });
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
        let metrics = IngestionMetrics::new(None, &registry);
        let mock_client = Arc::new(MockIngestionClient::default());
        let client = IngestionClient::new_impl(mock_client.clone(), metrics);
        (client, mock_client)
    }

    #[tokio::test]
    async fn test_fetch_raw_bytes_success() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint
        let bytes = Bytes::from(test_checkpoint_data(1));
        mock.checkpoints.insert(1, FetchData::Raw(bytes.clone()));

        // Fetch and verify
        let (result, _wire_size) = client.fetch(1).await.unwrap();
        assert_eq!(result.summary.sequence_number(), &1);
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_success() {
        let (client, mock) = setup_test();

        // Create test data - now returns zstd-compressed protobuf
        let bytes = Bytes::from(test_checkpoint_data(1));
        mock.checkpoints.insert(1, FetchData::Raw(bytes));

        // Fetch and verify
        let (result, _wire_size) = client.fetch(1).await.unwrap();
        assert_eq!(result.summary.sequence_number(), &1);
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

        // Create test data using test_checkpoint
        let bytes = Bytes::from(test_checkpoint_data(1));

        // Add checkpoint to mock with 2 transient failures
        mock.checkpoints.insert(1, FetchData::Raw(bytes));
        mock.transient_failures.insert(1, 2);

        // Fetch and verify it succeeds after retries
        let (result, _wire_size) = client.fetch(1).await.unwrap();
        assert_eq!(*result.summary.sequence_number(), 1);

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

        // Create test data - now returns zstd-compressed protobuf
        let bytes = Bytes::from(test_checkpoint_data(1));

        // Add checkpoint to mock with 1 not_found failures
        mock.checkpoints.insert(1, FetchData::Raw(bytes));
        mock.not_found_failures.insert(1, 1);

        // Wait for checkpoint with short retry interval
        let (result, _wire_size) = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.summary.sequence_number(), &1);

        // Verify that exactly 1 retry was recorded
        let retries = client.metrics.total_ingested_not_found_retries.get();
        assert_eq!(retries, 1);
    }

    #[tokio::test]
    async fn test_wait_for_checkpoint_instant() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint
        let bytes = Bytes::from(test_checkpoint_data(1));

        // Add checkpoint to mock with no failures - data should be available immediately
        mock.checkpoints.insert(1, FetchData::Raw(bytes));

        // Wait for checkpoint with short retry interval
        let (result, _wire_size) = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.summary.sequence_number(), &1);
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

    #[tokio::test]
    async fn test_fetch_non_retryable_error() {
        let (client, mock) = setup_test();

        mock.permanent_failures.insert(1, 1);

        let result = client.fetch(1).await;
        assert!(matches!(result, Err(IngestionError::FetchError(1, _))));

        // Verify that the non-retryable error metric was incremented
        let errors = client
            .metrics
            .total_ingested_permanent_errors
            .with_label_values(&["mock_permanent_error"])
            .get();
        assert_eq!(errors, 1);
    }
}
