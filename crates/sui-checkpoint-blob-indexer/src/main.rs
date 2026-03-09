// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use object_store::ClientOptions;
use object_store::RetryConfig;
use object_store::aws::{AmazonS3Builder, S3ConditionalPut};
use object_store::azure::MicrosoftAzureBuilder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::http::HttpBuilder;
use object_store::local::LocalFileSystem;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::service::Error;
use sui_indexer_alt_metrics::MetricsArgs;
use sui_indexer_alt_object_store::ObjectStore;
use tracing::info;
use url::Url;

use sui_checkpoint_blob_indexer::CheckpointBcsPipeline;
use sui_checkpoint_blob_indexer::CheckpointBlobPipeline;
use sui_checkpoint_blob_indexer::EpochsPipeline;
use sui_checkpoint_blob_indexer::IndexerConfig;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;

#[derive(Debug, Parser)]
#[command(name = "sui-checkpoint-blob-indexer")]
#[command(about = "Indexer that writes checkpoints as compressed proto blobs to object storage")]
#[group(id = "store", required = true, multiple = false)]
struct Args {
    /// Path to TOML config file
    #[arg(long)]
    config: PathBuf,

    /// Write to AWS S3. Provide the bucket name or endpoint-and-bucket.
    /// (env: AWS_ENDPOINT, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_DEFAULT_REGION)
    #[arg(long, group = "store")]
    s3: Option<String>,

    /// Write to Google Cloud Storage. Provide the bucket name.
    /// (env: GOOGLE_SERVICE_ACCOUNT_PATH)
    #[arg(long, group = "store")]
    gcs: Option<String>,

    /// Write to Azure Blob Storage. Provide the container name.
    /// (env: AZURE_STORAGE_ACCOUNT_NAME, AZURE_STORAGE_ACCESS_KEY)
    #[arg(long, group = "store")]
    azure: Option<String>,

    /// Write to HTTP endpoint.
    #[arg(long, group = "store")]
    http: Option<Url>,

    /// Write to local filesystem. Provide the path to the directory.
    #[arg(long, group = "store")]
    path: Option<PathBuf>,

    /// Request timeout
    #[arg(long, default_value = "30s", value_parser = humantime::parse_duration)]
    request_timeout: Duration,

    /// Optional Zstd compression level. If not provided, data will be stored uncompressed
    #[arg(long)]
    compression_level: Option<i32>,

    #[command(flatten)]
    metrics_args: MetricsArgs,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();

    let config_contents = tokio::fs::read_to_string(&args.config).await?;
    let config: IndexerConfig = toml::from_str(&config_contents)?;

    info!("Starting checkpoint object store indexer");
    info!("Args: {:#?}", args);
    info!("Config: {:#?}", config);

    let is_bounded_job = args.indexer_args.last_checkpoint.is_some();
    let client_options = ClientOptions::default().with_timeout(args.request_timeout);
    let retry_config = RetryConfig {
        max_retries: 0,
        ..Default::default()
    };

    let object_store: Arc<dyn object_store::ObjectStore> = if let Some(bucket) = args.s3 {
        info!(bucket, "Using S3 storage");
        AmazonS3Builder::from_env()
            .with_client_options(client_options)
            .with_retry(retry_config)
            .with_imdsv1_fallback()
            .with_bucket_name(bucket)
            .with_conditional_put(S3ConditionalPut::ETagMatch)
            .build()
            .map(Arc::new)?
    } else if let Some(bucket) = args.gcs {
        info!(bucket, "Using GCS storage");
        GoogleCloudStorageBuilder::from_env()
            .with_client_options(client_options)
            .with_retry(retry_config)
            .with_bucket_name(bucket)
            .build()
            .map(Arc::new)?
    } else if let Some(container) = args.azure {
        info!(container, "Using Azure storage");
        MicrosoftAzureBuilder::from_env()
            .with_client_options(client_options)
            .with_retry(retry_config)
            .with_container_name(container)
            .build()
            .map(Arc::new)?
    } else if let Some(endpoint) = args.http {
        info!(endpoint = %endpoint, "Using HTTP storage");
        HttpBuilder::new()
            .with_url(endpoint.to_string())
            .with_client_options(client_options)
            .with_retry(retry_config)
            .build()
            .map(Arc::new)?
    } else if let Some(path) = args.path {
        info!(path = %path.display(), "Using local filesystem storage");
        LocalFileSystem::new_with_prefix(path).map(Arc::new)?
    } else {
        unreachable!("clap ensures exactly one storage backend is provided");
    };

    let store = ObjectStore::new(object_store);

    let registry = prometheus::Registry::new_custom(Some("checkpoint_blob".into()), None)?;
    let metrics_service =
        sui_indexer_alt_metrics::MetricsService::new(args.metrics_args, registry.clone());

    let mut indexer = Indexer::new(
        store.clone(),
        args.indexer_args,
        args.client_args,
        config.ingestion.into(),
        None,
        &registry,
    )
    .await?;

    let committer = config.committer.finish(CommitterConfig::default());
    let base = ConcurrentConfig {
        committer,
        pruner: None,
        ..Default::default()
    };

    indexer
        .concurrent_pipeline(
            CheckpointBlobPipeline {
                compression_level: args.compression_level,
            },
            config.pipeline.checkpoint_blob.finish(base.clone()),
        )
        .await?;

    indexer
        .concurrent_pipeline(EpochsPipeline, config.pipeline.epochs.finish(base.clone()))
        .await?;

    indexer
        .concurrent_pipeline(
            CheckpointBcsPipeline,
            config.pipeline.checkpoint_bcs.finish(base),
        )
        .await?;

    let s_metrics = metrics_service.run().await?;
    let s_indexer = indexer.run().await?;

    match s_indexer.attach(s_metrics).main().await {
        Ok(()) => Ok(()),
        Err(Error::Terminated) => {
            if is_bounded_job {
                std::process::exit(1);
            } else {
                Ok(())
            }
        }
        Err(Error::Aborted) => {
            std::process::exit(1);
        }
        Err(Error::Task(_)) => {
            std::process::exit(2);
        }
    }
}
