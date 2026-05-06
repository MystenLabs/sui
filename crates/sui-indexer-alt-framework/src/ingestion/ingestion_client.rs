// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use backoff::Error as BE;
use backoff::ExponentialBackoff;
use backoff::backoff::Constant;
use clap::ArgGroup;
use mysten_network::callback::CallbackLayer;
use object_store::ClientOptions;
use object_store::ObjectStore;
use object_store::aws::AmazonS3Builder;
use object_store::azure::MicrosoftAzureBuilder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::http::HttpBuilder;
use object_store::local::LocalFileSystem;
use prometheus::Histogram;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;
use sui_futures::future::with_slow_future_monitor;
use sui_rpc::Client;
use sui_rpc::client::HeadersInterceptor;
use sui_types::digests::ChainIdentifier;
use tokio::sync::OnceCell;
use tracing::debug;
use tracing::warn;
use url::Url;

use crate::ingestion::Error as IE;
use crate::ingestion::MAX_GRPC_MESSAGE_SIZE_BYTES;
use crate::ingestion::Result as IngestionResult;
use crate::ingestion::byte_count::ByteCountMakeCallbackHandler;
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
    async fn chain_id(&self) -> anyhow::Result<ChainIdentifier>;

    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult;

    async fn latest_checkpoint_number(&self) -> anyhow::Result<u64>;
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

    /// GCP project ID for requester-pays GCS buckets. When set, the
    /// `x-goog-user-project` header is included in every request so that
    /// charges are billed to this project instead of the bucket owner.
    #[arg(long, requires = "remote_store_gcs")]
    pub remote_store_gcs_project_id: Option<String>,

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
            remote_store_gcs_project_id: None,
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
    #[error("Failed to fetch checkpoint: {0}")]
    Fetch(#[from] anyhow::Error),
    #[error("Failed to decode checkpoint: {0}")]
    Decode(#[from] decode::Error),
}

pub type CheckpointResult = Result<Checkpoint, CheckpointError>;

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
            let mut client_options = args.client_options();
            if let Some(project_id) = &args.remote_store_gcs_project_id {
                let header_value = HeaderValue::from_str(project_id)
                    .expect("invalid project ID for requester-pays header");
                let headers = HeaderMap::from_iter([
                    (
                        HeaderName::from_static("x-goog-user-project"),
                        header_value.clone(),
                    ),
                    (HeaderName::from_static("userproject"), header_value),
                ]);
                client_options = client_options.with_default_headers(headers);
            }
            let store = GoogleCloudStorageBuilder::from_env()
                .with_client_options(client_options)
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
        let client = Arc::new(StoreIngestionClient::new(
            store,
            Some(metrics.total_ingested_bytes.clone()),
        ));
        Ok(Self::new_impl(client, metrics))
    }

    /// An ingestion client that fetches checkpoints from a fullnode, over gRPC.
    pub fn with_grpc(
        url: Url,
        username: Option<String>,
        password: Option<String>,
        metrics: Arc<IngestionMetrics>,
    ) -> IngestionResult<Self> {
        let byte_count_layer = CallbackLayer::new(ByteCountMakeCallbackHandler::new(
            metrics.total_ingested_bytes.clone(),
        ));
        let client = Client::new(url.to_string())?
            .with_max_decoding_message_size(MAX_GRPC_MESSAGE_SIZE_BYTES)
            .request_layer(byte_count_layer);
        let client = if let Some(username) = username {
            let mut headers = HeadersInterceptor::new();
            headers.basic_auth(username, password);
            client.with_headers(headers)
        } else {
            client
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
                IE::NotFound(checkpoint) => {
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
    /// `MAX_TRANSIENT_RETRY_INTERVAL`). Transient errors are defined by the client
    /// implementation that returns a [CheckpointError::Fetch] or [CheckpointError::Decode] error
    /// variant.
    ///
    /// The function will immediately return if the checkpoint is not found.
    pub async fn checkpoint(&self, cp_sequence_number: u64) -> IngestionResult<CheckpointEnvelope> {
        let client = self.client.clone();
        let checkpoint_data_fut = retry_transient_with_slow_monitor(
            "checkpoint",
            move || {
                let client = client.clone();
                async move {
                    client
                        .checkpoint(cp_sequence_number)
                        .await
                        .map_err(|err| match err {
                            // Not found errors are marked as permanent here, but retried in
                            // `wait_for` in case the checkpoint becomes available in the future.
                            CheckpointError::NotFound => {
                                BE::permanent(IE::NotFound(cp_sequence_number))
                            }
                            // Retry fetch and decode errors in case the root cause is in the
                            // upstream checkpoint data source. If the upstream checkpoint data
                            // source is corrected, then the indexer will automatically recover
                            // the next time the read is attempted.
                            CheckpointError::Fetch(e) => self.metrics.inc_retry(
                                cp_sequence_number,
                                "fetch",
                                IE::FetchError(cp_sequence_number, e),
                            ),
                            CheckpointError::Decode(e) => self.metrics.inc_retry(
                                cp_sequence_number,
                                e.reason(),
                                IE::DecodeError(cp_sequence_number, e.into()),
                            ),
                        })
                }
            },
            &self.metrics.ingested_checkpoint_latency,
        );

        let client = self.client.clone();
        let chain_id_fut = self.chain_id.get_or_try_init(|| {
            retry_transient_with_slow_monitor(
                "chain_id",
                move || {
                    let client = client.clone();
                    async move {
                        client
                            .chain_id()
                            .await
                            .map_err(|e| BE::transient(IE::ChainIdError(cp_sequence_number, e)))
                    }
                },
                &self.metrics.ingested_chain_id_latency,
            )
        });

        let (checkpoint, chain_id) = tokio::try_join!(checkpoint_data_fut, chain_id_fut)?;

        self.checkpoint_lag_reporter
            .report_lag(cp_sequence_number, checkpoint.summary.timestamp_ms);

        self.metrics.total_ingested_checkpoints.inc();

        self.metrics
            .total_ingested_transactions
            .inc_by(checkpoint.transactions.len() as u64);

        self.metrics.total_ingested_events.inc_by(
            checkpoint
                .transactions
                .iter()
                .map(|tx| tx.events.as_ref().map_or(0, |evs| evs.data.len()) as u64)
                .sum(),
        );

        self.metrics
            .total_ingested_objects
            .inc_by(checkpoint.object_set.len() as u64);

        Ok(CheckpointEnvelope {
            checkpoint: Arc::new(checkpoint),
            chain_id: *chain_id,
        })
    }

    pub async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
        self.client.latest_checkpoint_number().await
    }
}

/// Keep backing off until we are waiting for the max interval, but don't give up.
pub(crate) fn transient_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        max_interval: MAX_TRANSIENT_RETRY_INTERVAL,
        max_elapsed_time: None,
        ..Default::default()
    }
}

/// Retry a fallible async operation with exponential backoff and slow-operation monitoring.
/// Records the total time (including retries) in the provided latency histogram.
pub(crate) async fn retry_transient_with_slow_monitor<F, Fut, T>(
    operation: &str,
    make_future: F,
    latency: &Histogram,
) -> IngestionResult<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, backoff::Error<IE>>>,
{
    let request = || {
        let fut = make_future();
        async move {
            with_slow_future_monitor(fut, SLOW_OPERATION_WARNING_THRESHOLD, || {
                warn!(
                    operation,
                    threshold_ms = SLOW_OPERATION_WARNING_THRESHOLD.as_millis(),
                    "Slow operation detected"
                );
            })
            .await
        }
    };

    let guard = latency.start_timer();
    let data = backoff::future::retry(transient_backoff(), request).await?;
    let elapsed = guard.stop_and_record();

    debug!(
        operation,
        elapsed_ms = elapsed * 1000.0,
        "Fetched operation"
    );

    Ok(data)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use clap::Parser;
    use clap::error::ErrorKind;
    use dashmap::DashMap;
    use prometheus::Registry;
    use sui_types::digests::CheckpointDigest;
    use sui_types::event::Event;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use crate::ingestion::decode;
    use crate::ingestion::test_utils::test_checkpoint_data;

    use super::*;

    fn test_checkpoint(seq: u64) -> Checkpoint {
        let bytes = test_checkpoint_data(seq);
        decode::checkpoint(&bytes).unwrap()
    }

    /// Build a checkpoint with one transaction containing one event and one created object.
    fn test_checkpoint_with_data(seq: u64) -> Checkpoint {
        TestCheckpointBuilder::new(seq)
            .start_transaction(0)
            .create_owned_object(0)
            .with_events(vec![Event::random_for_testing()])
            .finish_transaction()
            .build_checkpoint()
    }

    #[derive(Debug, Parser)]
    struct TestArgs {
        #[clap(flatten)]
        ingestion: IngestionClientArgs,
    }

    /// Mock implementation of IngestionClientTrait for testing.
    ///
    /// - `checkpoints`: pre-inserted checkpoints returned by `checkpoint()`. Sequences not
    ///   in the map return `CheckpointError::NotFound`.
    /// - `not_found_failures` / `fetch_failures` / `decode_failures`: number of times to
    ///   return the corresponding error for a given sequence number before succeeding.
    /// - `latest_checkpoint`: value returned by `latest_checkpoint_number()`.
    #[derive(Default)]
    pub(crate) struct MockIngestionClient {
        pub checkpoints: DashMap<u64, Checkpoint>,
        pub not_found_failures: DashMap<u64, usize>,
        pub fetch_failures: DashMap<u64, usize>,
        pub decode_failures: DashMap<u64, usize>,
        pub latest_checkpoint: u64,
    }

    impl MockIngestionClient {
        pub(crate) fn mock_chain_id() -> ChainIdentifier {
            CheckpointDigest::new([1; 32]).into()
        }

        /// Populate `checkpoints` with synthetic test checkpoints for the given sequence
        /// numbers.
        pub(crate) fn insert_checkpoints(&self, range: impl IntoIterator<Item = u64>) {
            for seq in range {
                self.checkpoints.insert(seq, test_checkpoint(seq));
            }
        }
    }

    #[async_trait]
    impl IngestionClientTrait for MockIngestionClient {
        async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
            Ok(Self::mock_chain_id())
        }

        async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
            if let Some(mut remaining) = self.not_found_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(CheckpointError::NotFound);
            }

            if let Some(mut remaining) = self.fetch_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(CheckpointError::Fetch(anyhow::anyhow!("Mock fetch error")));
            }

            if let Some(mut remaining) = self.decode_failures.get_mut(&checkpoint)
                && *remaining > 0
            {
                *remaining -= 1;
                return Err(CheckpointError::Decode(decode::Error::Deserialization(
                    prost::DecodeError::new("Mock deserialization error"),
                )));
            }

            self.checkpoints
                .get(&checkpoint)
                .as_deref()
                .cloned()
                .ok_or(CheckpointError::NotFound)
        }

        async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
            Ok(self.latest_checkpoint)
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
    async fn test_checkpoint_checkpoint_success() {
        let (client, mock) = setup_test();

        mock.checkpoints.insert(1, test_checkpoint_with_data(1));

        let result = client.checkpoint(1).await.unwrap();
        assert_eq!(result.checkpoint.summary.sequence_number(), &1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());
        assert_eq!(client.metrics.total_ingested_checkpoints.get(), 1);
        assert_eq!(client.metrics.total_ingested_transactions.get(), 1);
        assert_eq!(client.metrics.total_ingested_events.get(), 1);
        // 1 created object + 2 gas object versions (input + output)
        assert_eq!(client.metrics.total_ingested_objects.get(), 3);
    }

    #[tokio::test]
    async fn test_checkpoint_not_found() {
        let (client, _) = setup_test();

        // Try to fetch non-existent checkpoint
        let result = client.checkpoint(1).await;
        assert!(matches!(result, Err(IE::NotFound(1))));
        assert_eq!(client.metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(client.metrics.total_ingested_transactions.get(), 0);
        assert_eq!(client.metrics.total_ingested_events.get(), 0);
        assert_eq!(client.metrics.total_ingested_objects.get(), 0);
    }

    #[tokio::test]
    async fn test_checkpoint_fetch_error_with_retry() {
        let (client, mock) = setup_test();

        mock.checkpoints.insert(1, test_checkpoint(1));
        mock.fetch_failures.insert(1, 2);

        // Fetch and verify it succeeds after retries
        let result = client.checkpoint(1).await.unwrap();
        assert_eq!(*result.checkpoint.summary.sequence_number(), 1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());

        // Verify that exactly 2 retries were recorded
        let retries = client
            .metrics
            .total_ingested_transient_retries
            .with_label_values(&["fetch"])
            .get();
        assert_eq!(retries, 2);
        assert_eq!(client.metrics.total_ingested_checkpoints.get(), 1);
        assert_eq!(client.metrics.total_ingested_transactions.get(), 0);
        assert_eq!(client.metrics.total_ingested_events.get(), 0);
        assert_eq!(client.metrics.total_ingested_objects.get(), 0);
    }

    #[tokio::test]
    async fn test_checkpoint_decode_error_with_retry() {
        let (client, mock) = setup_test();

        mock.checkpoints.insert(1, test_checkpoint(1));
        mock.decode_failures.insert(1, 2);

        // Fetch and verify it succeeds after retries
        let result = client.checkpoint(1).await.unwrap();
        assert_eq!(*result.checkpoint.summary.sequence_number(), 1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());

        // Verify that exactly 2 retries were recorded
        let retries = client
            .metrics
            .total_ingested_transient_retries
            .with_label_values(&["deserialization"])
            .get();
        assert_eq!(retries, 2);
        assert_eq!(client.metrics.total_ingested_checkpoints.get(), 1);
        assert_eq!(client.metrics.total_ingested_transactions.get(), 0);
        assert_eq!(client.metrics.total_ingested_events.get(), 0);
        assert_eq!(client.metrics.total_ingested_objects.get(), 0);
    }

    #[tokio::test]
    async fn test_wait_for_checkpoint_with_retry() {
        let (client, mock) = setup_test();

        mock.checkpoints.insert(1, test_checkpoint(1));
        mock.not_found_failures.insert(1, 1);

        // Wait for checkpoint with short retry interval
        let result = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.checkpoint.summary.sequence_number(), &1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());

        // Verify that exactly 1 retry was recorded
        let retries = client.metrics.total_ingested_not_found_retries.get();
        assert_eq!(retries, 1);
        assert_eq!(client.metrics.total_ingested_checkpoints.get(), 1);
        assert_eq!(client.metrics.total_ingested_transactions.get(), 0);
        assert_eq!(client.metrics.total_ingested_events.get(), 0);
        assert_eq!(client.metrics.total_ingested_objects.get(), 0);
    }

    #[tokio::test]
    async fn test_wait_for_checkpoint_instant() {
        let (client, mock) = setup_test();

        mock.checkpoints.insert(1, test_checkpoint(1));

        let result = client.wait_for(1, Duration::from_millis(50)).await.unwrap();
        assert_eq!(result.checkpoint.summary.sequence_number(), &1);
        assert_eq!(result.chain_id, MockIngestionClient::mock_chain_id());
        assert_eq!(client.metrics.total_ingested_checkpoints.get(), 1);
        assert_eq!(client.metrics.total_ingested_transactions.get(), 0);
        assert_eq!(client.metrics.total_ingested_events.get(), 0);
        assert_eq!(client.metrics.total_ingested_objects.get(), 0);
    }
}
