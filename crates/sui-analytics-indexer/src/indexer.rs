// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Analytics indexer builder.

use std::sync::Arc;
use std::time::Duration;

use crate::package_store::PackageCache;
use anyhow::{Context, Result, anyhow};
use object_store::{
    ClientOptions, aws::AmazonS3Builder, azure::MicrosoftAzureBuilder,
    gcp::GoogleCloudStorageBuilder, local::LocalFileSystem,
};
use tracing::info;

use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_object_store::ObjectStore;

use crate::config::{IndexerConfig, OutputStoreConfig};
use crate::handlers::load_backfill_targets;
use crate::metrics::Metrics;

pub async fn build_analytics_indexer(
    config: IndexerConfig,
    metrics: Metrics,
    registry: prometheus::Registry,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Indexer<ObjectStore>> {
    let object_store = create_object_store(&config.output_store, config.request_timeout_secs)?;

    let store = ObjectStore::new(object_store.clone());

    let work_dir = if let Some(ref work_dir) = config.work_dir {
        tempfile::Builder::new()
            .prefix("sui-analytics-indexer-")
            .tempdir_in(work_dir)?
            .keep()
    } else {
        tempfile::Builder::new()
            .prefix("sui-analytics-indexer-")
            .tempdir()?
            .keep()
    };

    let package_cache_path = work_dir.join("package_cache");
    let package_cache = Arc::new(PackageCache::new(&package_cache_path, &config.rest_url));

    let indexer_args = sui_indexer_alt_framework::IndexerArgs {
        first_checkpoint: config.first_checkpoint,
        last_checkpoint: config.last_checkpoint,
        pipeline: vec![],
        task: config
            .task_name
            .as_ref()
            .map(|task_name| {
                sui_indexer_alt_framework::TaskArgs::tasked(
                    task_name.clone(),
                    config.reader_interval_ms,
                )
            })
            .unwrap_or_default(),
    };

    let client_args = sui_indexer_alt_framework::ingestion::ClientArgs {
        ingestion: sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs {
            remote_store_url: Some(url::Url::parse(&config.remote_store_url)?),
            local_ingestion_path: None,
            rpc_api_url: config
                .rpc_api_url
                .as_ref()
                .map(|url| url.parse())
                .transpose()?,
            rpc_username: config.rpc_username.clone(),
            rpc_password: config.rpc_password.clone(),
        },
        streaming: sui_indexer_alt_framework::ingestion::streaming_client::StreamingClientArgs {
            streaming_url: config
                .streaming_url
                .as_ref()
                .map(|url| url.parse())
                .transpose()?,
        },
    };

    let ingestion_config = config.ingestion.clone();
    let concurrent_config = config.concurrent.clone();

    let mut indexer = Indexer::new(
        store.clone(),
        indexer_args,
        client_args,
        ingestion_config,
        None,
        &registry,
        cancel.clone(),
    )
    .await?;

    // Validate backfill mode configuration
    if config.backfill_mode {
        if config.task_name.is_none() {
            return Err(anyhow!(
                "backfill_mode requires task_name to be set for watermark isolation"
            ));
        }
        if config.last_checkpoint.is_none() {
            return Err(anyhow!("backfill_mode requires last_checkpoint to be set"));
        }
    }

    for pipeline_config in config.pipeline_configs() {
        info!("Registering pipeline: {}", pipeline_config.pipeline);

        // Load backfill boundaries upfront if in backfill mode
        let backfill_boundaries = if config.backfill_mode {
            let first = config.first_checkpoint.unwrap_or(0);
            let last = config.last_checkpoint.expect("validated above");

            let targets = load_backfill_targets(
                object_store.clone(),
                pipeline_config.dir_prefix(),
                pipeline_config.file_format,
                first..last,
            )
            .await?;

            info!(
                pipeline = %pipeline_config.pipeline,
                file_count = targets.len(),
                "Loaded backfill targets"
            );

            Some(Arc::new(targets))
        } else {
            None
        };

        pipeline_config
            .pipeline
            .register(
                &mut indexer,
                pipeline_config,
                package_cache.clone(),
                metrics.clone(),
                concurrent_config.clone(),
                backfill_boundaries,
            )
            .await?;
    }
    Ok(indexer)
}

fn create_object_store(
    config: &OutputStoreConfig,
    timeout_secs: u64,
) -> Result<Arc<dyn object_store::ObjectStore>> {
    let client_options = ClientOptions::default().with_timeout(Duration::from_secs(timeout_secs));

    match config {
        OutputStoreConfig::Gcs {
            bucket,
            service_account_path,
        } => GoogleCloudStorageBuilder::new()
            .with_client_options(client_options)
            .with_bucket_name(bucket)
            .with_service_account_path(service_account_path.to_string_lossy())
            .build()
            .map(|s| Arc::new(s) as Arc<dyn object_store::ObjectStore>)
            .context("Failed to create GCS store"),
        OutputStoreConfig::S3 {
            bucket,
            region,
            access_key_id,
            secret_access_key,
            endpoint,
        } => {
            let mut builder = AmazonS3Builder::new()
                .with_client_options(client_options)
                .with_bucket_name(bucket)
                .with_region(region);
            if let Some(key) = access_key_id {
                builder = builder.with_access_key_id(key);
            }
            if let Some(secret) = secret_access_key {
                builder = builder.with_secret_access_key(secret);
            }
            if let Some(ep) = endpoint {
                builder = builder.with_endpoint(ep);
            }
            builder
                .build()
                .map(|s| Arc::new(s) as Arc<dyn object_store::ObjectStore>)
                .context("Failed to create S3 store")
        }
        OutputStoreConfig::Azure {
            container,
            account,
            access_key,
        } => MicrosoftAzureBuilder::new()
            .with_client_options(client_options)
            .with_container_name(container)
            .with_account(account)
            .with_access_key(access_key)
            .build()
            .map(|s| Arc::new(s) as Arc<dyn object_store::ObjectStore>)
            .context("Failed to create Azure store"),
        OutputStoreConfig::File { path } => LocalFileSystem::new_with_prefix(path)
            .map(|s| Arc::new(s) as Arc<dyn object_store::ObjectStore>)
            .context("Failed to create file store"),
    }
}
