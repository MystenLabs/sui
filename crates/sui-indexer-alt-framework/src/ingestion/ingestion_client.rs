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
use clap::ArgGroup;
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
use sui_types::digests::ChainIdentifier;
use tokio::sync::OnceCell;
use tracing::debug;
use tracing::error;
use tracing::warn;
use url::Url;

use crate::ingestion::Error as IngestionError;
use crate::ingestion::MAX_GRPC_MESSAGE_SIZE_BYTES;
use crate::ingestion::Result as IngestionResult;
use crate::ingestion::decode;
use crate::ingestion::error::Error::FetchError;
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
    async fn chain_id(&self) -> anyhow::Result<ChainIdentifier>;

    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult;
}

#[derive(clap::Args, Clone, Debug)]
#[command(group(ArgGroup::new("source").required(true).multiple(false)))]
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
    #[arg(long, env, requires = "rpc_api_url")]
    pub rpc_username: Option<String>,

    /// Optional password for the gRPC service.
    #[arg(long, env, requires = "rpc_api_url")]
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

impl IngestionClientArgs {
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
pub enum CheckpointError {
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

pub type CheckpointResult = Result<CheckpointData, CheckpointError>;

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum CheckpointData {
    Raw(Bytes),
    Checkpoint(Checkpoint),
}

#[derive(Clone)]
pub struct IngestionClient {
    client: Arc<dyn IngestionClientTrait>,
    /// Wrap the metrics in an `Arc` to keep copies of the client cheap.
    metrics: Arc<IngestionMetrics>,
    checkpoint_lag_reporter: Arc<CheckpointLagMetricReporter>,
    chain_id: OnceCell<ChainIdentifier>,
}

#[derive(Clone, Debug)]
pub struct CheckpointEnvelope {
    pub checkpoint: Arc<Checkpoint>,
    pub chain_id: ChainIdentifier,
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
            chain_id: OnceCell::new(),
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
    ) -> IngestionResult<CheckpointEnvelope> {
        let backoff = Constant::new(retry_interval);
        let fetch = || async move {
            use backoff::Error as BE;
            self.checkpoint(checkpoint).await.map_err(|e| match e {
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
    /// implementation that returns a [CheckpointError::Transient] error variant, or within
    /// this function if we fail to deserialize the result as [Checkpoint].
    ///
    /// The function will immediately return if the checkpoint is not found.
    pub async fn checkpoint(&self, checkpoint: u64) -> IngestionResult<CheckpointEnvelope> {
        let checkpoint_client = self.client.clone();
        let request = move || {
            let client = checkpoint_client.clone();
            async move {
                let checkpoint_data = with_slow_future_monitor(
                    client.checkpoint(checkpoint),
                    SLOW_OPERATION_WARNING_THRESHOLD,
                    /* on_threshold_exceeded =*/
                    || {
                        warn!(
                            checkpoint,
                            threshold_ms = SLOW_OPERATION_WARNING_THRESHOLD.as_millis(),
                            "Slow checkpoint operation detected"
                        );
                    },
                )
                .await
                .map_err(|err| match err {
                    CheckpointError::NotFound => {
                        BE::permanent(IngestionError::NotFound(checkpoint))
                    }
                    CheckpointError::Transient { reason, error } => self.metrics.inc_retry(
                        checkpoint,
                        reason,
                        IngestionError::FetchError(checkpoint, error),
                    ),
                    CheckpointError::Permanent { reason, error } => {
                        error!(checkpoint, reason, "Permanent checkpoint error: {error}");
                        self.metrics
                            .total_ingested_permanent_errors
                            .with_label_values(&[reason])
                            .inc();
                        BE::permanent(IngestionError::FetchError(checkpoint, error))
                    }
                })?;

                Ok::<Checkpoint, backoff::Error<IngestionError>>(match checkpoint_data {
                    CheckpointData::Raw(bytes) => {
                        self.metrics.total_ingested_bytes.inc_by(bytes.len() as u64);

                        decode::checkpoint(&bytes).map_err(|e| {
                            self.metrics.inc_retry(
                                checkpoint,
                                e.reason(),
                                IngestionError::DeserializationError(checkpoint, e.into()),
                            )
                        })?
                    }
                    CheckpointData::Checkpoint(data) => {
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

        let chain_id = *self.get_or_init_chain_id(checkpoint).await?;

        Ok(CheckpointEnvelope {
            checkpoint: Arc::new(data),
            chain_id,
        })
    }

    async fn get_or_init_chain_id(&self, checkpoint: u64) -> IngestionResult<&ChainIdentifier> {
        let chain_id_client = self.client.clone();
        let chain_id = self
            .chain_id
            .get_or_try_init(|| {
                let request = move || {
                    let client = chain_id_client.clone();
                    async move {
                        let chain_id = with_slow_future_monitor(
                            client.chain_id(),
                            SLOW_OPERATION_WARNING_THRESHOLD,
                            /* on_threshold_exceeded =*/
                            || {
                                warn!(
                                    checkpoint,
                                    threshold_ms = SLOW_OPERATION_WARNING_THRESHOLD.as_millis(),
                                    "Slow chain_id operation detected"
                                );
                            },
                        )
                        .await
                        .map_err(|err| {
                            let reason = "chain_id";
                            warn!(reason, "Retrying due to error: {err}");
                            backoff::Error::transient(FetchError(checkpoint, err))
                        })?;

                        Ok::<ChainIdentifier, backoff::Error<IngestionError>>(chain_id)
                    }
                };

                // Keep backing off until we are waiting for the max interval, but don't give up.
                let backoff = ExponentialBackoff {
                    max_interval: MAX_TRANSIENT_RETRY_INTERVAL,
                    max_elapsed_time: None,
                    ..Default::default()
                };

                backoff::future::retry(backoff, request)
            })
            .await?;
        Ok(chain_id)
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use clap::error::ErrorKind;
    use dashmap::DashMap;
    use prometheus::Registry;
    use std::sync::Arc;
    use std::time::Duration;
    use sui_types::digests::CheckpointDigest;
    use tokio::time::timeout;

    use crate::ingestion::test_utils::test_checkpoint_data;

    use super::*;

    #[derive(Debug, Parser)]
    struct TestArgs {
        #[clap(flatten)]
        ingestion: IngestionClientArgs,
    }

    /// Mock implementation of IngestionClientTrait for testing
    #[derive(Default)]
    struct MockIngestionClient {
        checkpoints: DashMap<u64, CheckpointData>,
        transient_failures: DashMap<u64, usize>,
        not_found_failures: DashMap<u64, usize>,
        permanent_failures: DashMap<u64, usize>,
    }

    impl MockIngestionClient {
        fn mock_chain_id() -> ChainIdentifier {
            CheckpointDigest::new([1; 32]).into()
        }
    }

    #[async_trait]
    impl IngestionClientTrait for MockIngestionClient {
        async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
            Ok(Self::mock_chain_id())
        }

        async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
            // Check for not found failures
            if let Some(mut remaining) = self.not_found_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(CheckpointError::NotFound);
            }

            // Check for non-retryable failures
            if let Some(mut remaining) = self.permanent_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(CheckpointError::Permanent {
                    reason: "mock_permanent_error",
                    error: anyhow::anyhow!("Mock permanent error"),
                });
            }

            // Check for transient failures
            if let Some(mut remaining) = self.transient_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(CheckpointError::Transient {
                    reason: "mock_transient_error",
                    error: anyhow::anyhow!("Mock transient error"),
                });
            }

            // Return the checkpoint data if it exists
            self.checkpoints
                .get(&checkpoint)
                .as_deref()
                .cloned()
                .ok_or(CheckpointError::NotFound)
        }
    }

    fn setup_test() -> (IngestionClient, Arc<MockIngestionClient>) {
        let registry = Registry::new_custom(Some("test".to_string()), None).unwrap();
        let metrics = IngestionMetrics::new(None, &registry);
        let mock_client = Arc::new(MockIngestionClient::default());
        let client = IngestionClient::new_impl(mock_client.clone(), metrics);
        (client, mock_client)
    }

    #[test]
    fn test_args_multiple_ingestion_sources_are_rejected() {
        let err = TestArgs::try_parse_from([
            "cmd",
            "--remote-store-url",
            "https://example.com",
            "--rpc-api-url",
            "http://localhost:8080",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }

    #[test]
    fn test_args_optional_credentials() {
        let args = TestArgs::try_parse_from([
            "cmd",
            "--rpc-api-url",
            "http://localhost:8080",
            "--rpc-username",
            "alice",
            "--rpc-password",
            "secret",
        ])
        .unwrap();

        assert_eq!(args.ingestion.rpc_username.as_deref(), Some("alice"));
        assert_eq!(args.ingestion.rpc_password.as_deref(), Some("secret"));
        assert_eq!(
            args.ingestion.rpc_api_url,
            Some(Url::parse("http://localhost:8080").unwrap())
        );
    }

    #[test]
    fn test_args_credentials_require_rpc_url() {
        let err = TestArgs::try_parse_from([
            "cmd",
            "--rpc-username",
            "alice",
            "--rpc-password",
            "secret",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[tokio::test]
    async fn test_fetch_raw_bytes_success() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint
        let bytes = Bytes::from(test_checkpoint_data(1));
        mock.checkpoints
            .insert(1, CheckpointData::Raw(bytes.clone()));

        // Fetch and verify
        let result = client.checkpoint(1).await.unwrap();
        assert_eq!(result.checkpoint.summary.sequence_number(), &1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());
    }

    #[tokio::test]
    async fn test_fetch_checkpoint_success() {
        let (client, mock) = setup_test();

        // Create test data - now returns zstd-compressed protobuf
        let bytes = Bytes::from(test_checkpoint_data(1));
        mock.checkpoints.insert(1, CheckpointData::Raw(bytes));

        // Fetch and verify
        let result = client.checkpoint(1).await.unwrap();
        assert_eq!(result.checkpoint.summary.sequence_number(), &1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());
    }

    #[tokio::test]
    async fn test_fetch_not_found() {
        let (client, _) = setup_test();

        // Try to fetch non-existent checkpoint
        let result = client.checkpoint(1).await;
        assert!(matches!(result, Err(IngestionError::NotFound(1))));
    }

    #[tokio::test]
    async fn test_fetch_transient_error_with_retry() {
        let (client, mock) = setup_test();

        // Create test data using test_checkpoint
        let bytes = Bytes::from(test_checkpoint_data(1));

        // Add checkpoint to mock with 2 transient failures
        mock.checkpoints.insert(1, CheckpointData::Raw(bytes));
        mock.transient_failures.insert(1, 2);

        // Fetch and verify it succeeds after retries
        let result = client.checkpoint(1).await.unwrap();
        assert_eq!(*result.checkpoint.summary.sequence_number(), 1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());

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
        mock.checkpoints.insert(1, CheckpointData::Raw(bytes));
        mock.not_found_failures.insert(1, 1);

        // Wait for checkpoint with short retry interval
        let result = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.checkpoint.summary.sequence_number(), &1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());

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
        mock.checkpoints.insert(1, CheckpointData::Raw(bytes));

        // Wait for checkpoint with short retry interval
        let result = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.checkpoint.summary.sequence_number(), &1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());
    }

    #[tokio::test]
    async fn test_wait_for_permanent_deserialization_error() {
        let (client, mock) = setup_test();

        // Add invalid data that will cause a deserialization error
        mock.checkpoints
            .insert(1, CheckpointData::Raw(Bytes::from("invalid data")));

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

        let result = client.checkpoint(1).await;
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
