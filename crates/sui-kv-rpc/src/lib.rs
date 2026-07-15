// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::ensure;
use prometheus::HistogramVec;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use sui_kvstore::ALPHA_PIPELINE_NAMES;
use sui_kvstore::BigTableClient;
use sui_kvstore::CHECKPOINTS_BY_DIGEST_PIPELINE;
use sui_kvstore::CHECKPOINTS_PIPELINE;
use sui_kvstore::EPOCH_END_PIPELINE;
use sui_kvstore::EPOCH_START_PIPELINE;
use sui_kvstore::KeyValueStoreReader;
use sui_kvstore::OBJECTS_PIPELINE;
pub use sui_kvstore::PoolConfig;
use sui_kvstore::TRANSACTIONS_PIPELINE;
use sui_package_resolver::PackageStore;
use sui_package_resolver::PackageStoreWithLruCache;
use sui_package_resolver::Resolver;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerServiceServer;
use sui_rpc_api::ServerVersion;
use sui_types::digests::ChainIdentifier;
use sui_types::message_envelope::Message;
use tokio::sync::RwLock;
use tokio::time::Duration;
use tokio::time::sleep;
use tonic::transport::Identity;
use tonic::transport::Server;
use tonic::transport::ServerTlsConfig;
use tracing::error;

mod bigtable_client;
mod config;
mod object_cache;
mod operation;
mod package_store;
mod pipeline;
mod render;
mod resolve;
mod v2;

use bigtable_client::Metrics as BigTableLimiterMetrics;
pub use config::KvRpcConfig;
pub use config::LedgerHistoryConfig;
pub use config::LedgerHistoryMethodConfig;
pub use config::PipelineStage;
pub use config::ResolvedLedgerHistoryMethodConfig;
pub use config::ResolvedStageConfig;
pub use config::StageConfig;
pub use config::StagesConfig;
use package_store::BigTablePackageStore;

pub const DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES: [&str; 6] = [
    CHECKPOINTS_PIPELINE,
    CHECKPOINTS_BY_DIGEST_PIPELINE,
    TRANSACTIONS_PIPELINE,
    OBJECTS_PIPELINE,
    EPOCH_START_PIPELINE,
    EPOCH_END_PIPELINE,
];

pub const EXPERIMENTAL_QUERY_SERVICE_INFO_WATERMARK_PIPELINES: [&str; 3] = ALPHA_PIPELINE_NAMES;

pub type PackageResolver = Arc<Resolver<Arc<dyn PackageStore>>>;

#[derive(Clone)]
pub(crate) struct KvRpcMetrics {
    bigtable_limiter: Arc<BigTableLimiterMetrics>,
    response_render_latency_ms: HistogramVec,
    stream_item_yield_wait_ms: HistogramVec,
    bitmap_buckets_evaluated: HistogramVec,
    bitmap_buckets_discarded: HistogramVec,
    bitmap_leaf_seeks: HistogramVec,
}

impl KvRpcMetrics {
    fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            bigtable_limiter: BigTableLimiterMetrics::new(registry),
            response_render_latency_ms: register_histogram_vec_with_registry!(
                "kv_rpc_response_render_latency_ms",
                "Wall time spent rendering one list response item.",
                &["method"],
                prometheus::exponential_buckets(0.01, 2.0, 18).unwrap(),
                registry,
            )
            .unwrap(),
            stream_item_yield_wait_ms: register_histogram_vec_with_registry!(
                "kv_rpc_stream_item_yield_wait_ms",
                "Wall time from yielding one list response item until the stream is polled again.",
                &["method"],
                prometheus::exponential_buckets(0.01, 2.0, 18).unwrap(),
                registry,
            )
            .unwrap(),
            bitmap_buckets_evaluated: register_histogram_vec_with_registry!(
                "kv_rpc_bitmap_buckets_evaluated",
                "Buckets evaluated against the per-request bitmap scan budget. Caps at the configured budget; actual backend bucket reads may exceed it by up to max_bitmap_filter_literals (one extra fetched-and-discarded bucket per leaf stream at exhaustion). Tail near the per-request cap means clients are hitting SCAN_LIMIT and paging.",
                &["method"],
                // Exponential 1..2048: many requests evaluate few buckets
                // (small or empty scans), a tail saturates at the default 1k cap.
                prometheus::exponential_buckets(1.0, 2.0, 12).unwrap(),
                registry,
            )
            .unwrap(),
            bitmap_buckets_discarded: register_histogram_vec_with_registry!(
                "kv_rpc_bitmap_buckets_discarded",
                "Charged dead bucket rows discarded while bitmap leaves catch up across provably unmatchable gaps.",
                &["method"],
                prometheus::exponential_buckets(1.0, 2.0, 12).unwrap(),
                registry,
            )
            .unwrap(),
            bitmap_leaf_seeks: register_histogram_vec_with_registry!(
                "kv_rpc_bitmap_leaf_seeks",
                "Bitmap leaf scans abandoned and reopened beyond provably unmatchable gaps.",
                &["method"],
                prometheus::exponential_buckets(1.0, 2.0, 12).unwrap(),
                registry,
            )
            .unwrap(),
        })
    }

    fn observe_response_render(&self, method: &'static str, elapsed: std::time::Duration) {
        self.response_render_latency_ms
            .with_label_values(&[method])
            .observe(elapsed.as_secs_f64() * 1000.0);
    }

    fn observe_stream_item_yield_wait(&self, method: &'static str, elapsed: std::time::Duration) {
        self.stream_item_yield_wait_ms
            .with_label_values(&[method])
            .observe(elapsed.as_secs_f64() * 1000.0);
    }

    fn observe_bitmap_buckets_evaluated(&self, method: &'static str, buckets: u64) {
        self.bitmap_buckets_evaluated
            .with_label_values(&[method])
            .observe(buckets as f64);
    }
    fn observe_bitmap_buckets_discarded(&self, method: &'static str, buckets: u64) {
        self.bitmap_buckets_discarded
            .with_label_values(&[method])
            .observe(buckets as f64);
    }

    fn observe_bitmap_leaf_seeks(&self, method: &'static str, seeks: u64) {
        self.bitmap_leaf_seeks
            .with_label_values(&[method])
            .observe(seeks as f64);
    }
}

#[derive(Clone)]
pub struct KvRpcServer {
    chain_id: ChainIdentifier,
    client: BigTableClient,
    server_version: Option<ServerVersion>,
    service_info_watermark_pipelines: Vec<&'static str>,
    cache: Arc<RwLock<Option<GetServiceInfoResponse>>>,
    package_resolver: PackageResolver,
    metrics: Arc<KvRpcMetrics>,
    pub(crate) ledger_history: LedgerHistoryConfig,
    pub(crate) request_bigtable_concurrency: usize,
    pub(crate) stages: StagesConfig,
    // The list RPCs are part of the stable v2 LedgerService, but serving them
    // requires the experimental query indexing pipelines. Instances without
    // them reject list requests with `Unimplemented`, matching the behavior
    // from when the RPCs lived in a separately registered v2alpha service.
    query_apis_enabled: bool,
}

/// Optional configuration for the gRPC server (TLS, metrics, reflection).
#[derive(Default)]
pub struct ServerConfig {
    pub tls_identity: Option<Identity>,
    pub metrics_registry: Option<Registry>,
    pub enable_reflection: bool,
    pub enable_experimental_query_apis: bool,
}

impl KvRpcServer {
    pub async fn new(
        instance_id: String,
        project_id: Option<String>,
        app_profile_id: Option<String>,
        channel_timeout: Option<Duration>,
        server_version: Option<ServerVersion>,
        registry: &Registry,
        credentials_path: Option<String>,
        pool_config: PoolConfig,
        service_info_watermark_pipelines: Vec<&'static str>,
        ledger_history: LedgerHistoryConfig,
        request_bigtable_concurrency: usize,
        stages: StagesConfig,
    ) -> anyhow::Result<Self> {
        ledger_history.validate()?;
        let mut client = BigTableClient::new_remote_with_credentials(
            instance_id,
            project_id,
            false,
            channel_timeout,
            None,
            "sui-kv-rpc".to_string(),
            Some(registry),
            app_profile_id,
            pool_config,
            credentials_path,
        )
        .await?;
        let genesis = client
            .get_checkpoints(&[0])
            .await?
            .pop()
            .expect("failed to fetch genesis checkpoint from the KV store");
        let summary = genesis.summary.expect("genesis checkpoint missing summary");
        let chain_id = ChainIdentifier::from(summary.digest());
        let metrics = KvRpcMetrics::new(registry);
        Self::init(
            client,
            chain_id,
            server_version,
            service_info_watermark_pipelines,
            metrics,
            ledger_history,
            request_bigtable_concurrency,
            stages,
        )
    }

    /// Construct a KvRpcServer backed by a local BigTable emulator.
    pub async fn new_local(
        host: String,
        instance_id: String,
        server_version: Option<ServerVersion>,
    ) -> anyhow::Result<Self> {
        Self::new_local_with_config(
            host,
            instance_id,
            server_version,
            LedgerHistoryConfig::default(),
            KvRpcConfig::default().request_bigtable_concurrency(),
            StagesConfig::default(),
        )
        .await
    }

    /// Construct a KvRpcServer backed by a local BigTable emulator with explicit
    /// ledger-history and per-stage configuration. Tests use this to exercise
    /// scan-budget / chunk-size behaviour (e.g. a small `tx_seq_digest` chunk
    /// size) against a modest synthetic dataset.
    pub async fn new_local_with_config(
        host: String,
        instance_id: String,
        server_version: Option<ServerVersion>,
        ledger_history: LedgerHistoryConfig,
        request_bigtable_concurrency: usize,
        stages: StagesConfig,
    ) -> anyhow::Result<Self> {
        let client = BigTableClient::new_local(host, instance_id).await?;
        // Emulator/test path: metrics are inert (no scrape endpoint), but the
        // request-scoped BigTable wrapper still expects a handle.
        let metrics = KvRpcMetrics::new(&Registry::default());
        Self::init(
            client,
            ChainIdentifier::from(sui_types::digests::CheckpointDigest::default()),
            server_version,
            default_service_info_watermark_pipelines(false),
            metrics,
            ledger_history,
            request_bigtable_concurrency,
            stages,
        )
    }

    fn init(
        client: BigTableClient,
        chain_id: ChainIdentifier,
        server_version: Option<ServerVersion>,
        service_info_watermark_pipelines: Vec<&'static str>,
        metrics: Arc<KvRpcMetrics>,
        ledger_history: LedgerHistoryConfig,
        request_bigtable_concurrency: usize,
        stages: StagesConfig,
    ) -> anyhow::Result<Self> {
        ensure!(
            !service_info_watermark_pipelines.is_empty(),
            "at least one service info watermark pipeline must be configured"
        );
        ledger_history.validate()?;

        let cache = Arc::new(RwLock::new(None));

        let package_store: Arc<dyn PackageStore> = Arc::new(PackageStoreWithLruCache::new(
            BigTablePackageStore::new(client.clone()),
        ));
        let package_resolver = Arc::new(Resolver::new(package_store));

        let server = Self {
            chain_id,
            client,
            server_version,
            service_info_watermark_pipelines,
            cache,
            package_resolver,
            metrics,
            ledger_history,
            request_bigtable_concurrency,
            stages,
            query_apis_enabled: false,
        };

        let server_clone = server.clone();
        tokio::spawn(async move {
            loop {
                match v2::get_service_info(
                    server_clone.client.clone(),
                    server_clone.chain_id,
                    server_clone.server_version.clone(),
                    &server_clone.service_info_watermark_pipelines,
                )
                .await
                {
                    Ok(info) => {
                        let mut cache = server_clone.cache.write().await;
                        *cache = Some(info);
                    }
                    Err(e) => error!("Failed to update service info cache: {:?}", e),
                }
                sleep(Duration::from_millis(10)).await;
            }
        });

        Ok(server)
    }

    pub(crate) fn check_query_apis_enabled(&self) -> Result<(), tonic::Status> {
        if self.query_apis_enabled {
            Ok(())
        } else {
            Err(tonic::Status::unimplemented(
                "the List APIs are not enabled on this instance",
            ))
        }
    }

    /// Start this server as a tonic gRPC service on the given address.
    /// Returns a `Service` handle for lifecycle management.
    pub async fn start_service(
        mut self,
        listen_address: SocketAddr,
        config: ServerConfig,
    ) -> anyhow::Result<sui_futures::service::Service> {
        use mysten_network::callback::CallbackLayer;
        use sui_rpc_api::{
            RpcMetrics, RpcMetricsMakeCallbackHandler, grpc_method_paths_from_file_descriptor_sets,
        };

        let mut builder = Server::builder();

        if let Some(identity) = config.tls_identity {
            builder = builder.tls_config(ServerTlsConfig::new().identity(identity))?;
        }

        // Single source of truth for every encoded FileDescriptorSet that
        // backs a gRPC service mounted below. Consumed by both the
        // reflection services and the metrics allowlist so they cannot drift
        // out of sync.
        self.query_apis_enabled = config.enable_experimental_query_apis;
        let file_descriptor_sets: Vec<&'static [u8]> = vec![
            sui_rpc_api::proto::google::protobuf::FILE_DESCRIPTOR_SET,
            sui_rpc_api::proto::google::rpc::FILE_DESCRIPTOR_SET,
            sui_rpc::proto::sui::rpc::v2::FILE_DESCRIPTOR_SET,
        ];

        let registry = config.metrics_registry.unwrap_or_default();
        let grpc_method_allowlist = Arc::new(grpc_method_paths_from_file_descriptor_sets(
            &file_descriptor_sets,
        )?);
        let mut router = builder
            .layer(CallbackLayer::new(
                RpcMetricsMakeCallbackHandler::with_grpc_method_allowlist(
                    Arc::new(RpcMetrics::new(&registry)),
                    grpc_method_allowlist,
                ),
            ))
            .add_service(LedgerServiceServer::new(self));

        if config.enable_reflection {
            let mut reflection_v1_builder = tonic_reflection::server::Builder::configure();
            let mut reflection_v1alpha_builder = tonic_reflection::server::Builder::configure();
            for &fds in &file_descriptor_sets {
                reflection_v1_builder =
                    reflection_v1_builder.register_encoded_file_descriptor_set(fds);
                reflection_v1alpha_builder =
                    reflection_v1alpha_builder.register_encoded_file_descriptor_set(fds);
            }
            router = router
                .add_service(reflection_v1_builder.build_v1()?)
                .add_service(reflection_v1alpha_builder.build_v1alpha()?);
        }

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let server_future = router.serve_with_shutdown(listen_address, async {
            let _ = rx.await;
        });

        let service = sui_futures::service::Service::new()
            .spawn(async move {
                server_future.await?;
                Ok(())
            })
            .with_shutdown_signal(async move {
                let _ = tx.send(());
            });

        Ok(service)
    }
}

pub fn default_service_info_watermark_pipelines(
    enable_experimental_query_apis: bool,
) -> Vec<&'static str> {
    let mut pipelines = DEFAULT_SERVICE_INFO_WATERMARK_PIPELINES.to_vec();
    if enable_experimental_query_apis {
        pipelines.extend_from_slice(&EXPERIMENTAL_QUERY_SERVICE_INFO_WATERMARK_PIPELINES);
    }
    pipelines
}
