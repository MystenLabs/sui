// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{path::PathBuf, sync::Arc, time::Duration};

use object_store::{
    ClientOptions, aws::AmazonS3Builder, azure::MicrosoftAzureBuilder,
    gcp::GoogleCloudStorageBuilder, http::HttpBuilder, local::LocalFileSystem,
};
use sui_checkpoint_blob_indexer::{CheckpointBlobPipeline, EpochsPipeline};
use sui_indexer_alt_framework::{Indexer, IndexerArgs, ingestion::ClientArgs};
use sui_indexer_alt_metrics::MetricsArgs;
use sui_indexer_alt_object_store::ObjectStore;
use url::Url;

#[derive(Debug, clap::Parser)]
#[command(name = "sui-checkpoint-blob-indexer")]
#[command(about = "Indexer that writes checkpoints as compressed proto blobs to object storage")]
#[group(id = "store", required = true, multiple = false)]
struct Args {
    /// Number of concurrent checkpoint uploads
    #[arg(long, default_value = "10")]
    write_concurrency: usize,

    /// Interval between watermark updates
    #[arg(long, default_value = "1m", value_parser = humantime::parse_duration)]
    watermark_interval: Duration,

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

    /// Optional watermark task name to override the watermark path
    #[arg(long)]
    watermark_task: Option<String>,

    #[command(flatten)]
    metrics_args: MetricsArgs,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use sui_indexer_alt_framework::{
        ingestion::IngestionConfig,
        pipeline::{CommitterConfig, concurrent::ConcurrentConfig},
    };
    use tracing::info;

    let args = Args::parse();

    tracing_subscriber::fmt::init();

    info!("Starting checkpoint object store indexer");
    info!("Args: {:#?}", args);

    let is_bounded_job = args.indexer_args.last_checkpoint.is_some();
    let client_options = ClientOptions::default().with_timeout(args.request_timeout);

    let object_store: Arc<dyn object_store::ObjectStore> = if let Some(bucket) = args.s3 {
        info!(bucket, "Using S3 storage");
        AmazonS3Builder::from_env()
            .with_client_options(client_options)
            .with_imdsv1_fallback()
            .with_bucket_name(bucket)
            .build()
            .map(Arc::new)?
    } else if let Some(bucket) = args.gcs {
        info!(bucket, "Using GCS storage");
        GoogleCloudStorageBuilder::from_env()
            .with_client_options(client_options)
            .with_bucket_name(bucket)
            .build()
            .map(Arc::new)?
    } else if let Some(container) = args.azure {
        info!(container, "Using Azure storage");
        MicrosoftAzureBuilder::from_env()
            .with_client_options(client_options)
            .with_container_name(container)
            .build()
            .map(Arc::new)?
    } else if let Some(endpoint) = args.http {
        info!(endpoint = %endpoint, "Using HTTP storage");
        HttpBuilder::new()
            .with_url(endpoint.to_string())
            .with_client_options(client_options)
            .build()
            .map(Arc::new)?
    } else if let Some(path) = args.path {
        info!(path = %path.display(), "Using local filesystem storage");
        LocalFileSystem::new_with_prefix(path).map(Arc::new)?
    } else {
        unreachable!("clap ensures exactly one storage backend is provided");
    };

    let store = ObjectStore::new(object_store);

    let cancel = tokio_util::sync::CancellationToken::new();

    let registry = prometheus::Registry::new_custom(Some("checkpoint_blob".into()), None)?;
    let metrics_service = sui_indexer_alt_metrics::MetricsService::new(
        args.metrics_args,
        registry.clone(),
        cancel.clone(),
    );

    let config = ConcurrentConfig {
        committer: CommitterConfig {
            write_concurrency: args.write_concurrency,
            watermark_interval_ms: args.watermark_interval.as_millis() as u64,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut indexer = Indexer::new(
        store.clone(),
        args.indexer_args,
        args.client_args,
        IngestionConfig::default(),
        None,
        &registry,
        cancel.clone(),
    )
    .await?;

    indexer
        .concurrent_pipeline(
            CheckpointBlobPipeline {
                compression_level: args.compression_level,
            },
            config.clone(),
        )
        .await?;

    indexer
        .concurrent_pipeline(EpochsPipeline, config.clone())
        .await?;

    let h_metrics = metrics_service.run().await?;
    let mut h_indexer = indexer.run().await?;

    enum ExitReason {
        Completed,
        UserInterrupt, // SIGINT / Ctrl-C
        Terminated,    // SIGTERM (i.e. from K8s)
    }

    let exit_reason = tokio::select! {
        res = &mut h_indexer => {
            tracing::info!("Indexer completed successfully");
            res?;
            ExitReason::Completed
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT, shutting down...");
            ExitReason::UserInterrupt
        }
        _ = wait_for_sigterm() => {
            tracing::info!("Received SIGTERM, shutting down...");
            ExitReason::Terminated
        }
    };

    cancel.cancel();
    tracing::info!("Waiting for graceful shutdown...");
    let _ = h_indexer.await;
    let _ = h_metrics.await;

    // Determine exit code based on exit reason and job type
    match exit_reason {
        ExitReason::Completed => {
            // Job finished all work successfully
            Ok(())
        }
        ExitReason::UserInterrupt => {
            // User manually stopped it - treat as success
            Ok(())
        }
        ExitReason::Terminated if is_bounded_job => {
            // Bounded job interrupted by K8s - work incomplete, trigger restart
            std::process::exit(1);
        }
        ExitReason::Terminated => {
            // Continuous indexer - normal shutdown
            Ok(())
        }
    }
}

#[cfg(unix)]
async fn wait_for_sigterm() {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("Failed to install SIGTERM handler")
        .recv()
        .await;
}

#[cfg(not(unix))]
async fn wait_for_sigterm() {
    // SIGTERM doesn't exist on Windows, so wait forever
    std::future::pending::<()>().await
}
