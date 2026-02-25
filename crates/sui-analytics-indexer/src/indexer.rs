// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Analytics indexer builder.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use object_store::ClientOptions;
use object_store::aws::AmazonS3Builder;
use object_store::azure::MicrosoftAzureBuilder;
use object_store::gcp::GoogleCloudStorageBuilder;
use object_store::local::LocalFileSystem;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;
use tokio_util::sync::CancellationToken;
use tracing::info;

use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::sequential::SequentialConfig;
use sui_indexer_alt_framework::service::Service;

use crate::config::IndexerConfig;
use crate::config::OutputStoreConfig;
use crate::metrics::Metrics;
use crate::package_store::PackageCache;
use crate::progress_monitoring::spawn_snowflake_monitors;
use crate::store::AnalyticsStore;

/// Build and run an analytics indexer, returning a Service handle.
///
/// The returned Service integrates store shutdown - when the service shuts down
/// gracefully, it will wait for all pending uploads to complete.
pub async fn build_analytics_indexer(
    config: IndexerConfig,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    metrics: Metrics,
    registry: prometheus::Registry,
) -> Result<Service> {
    // Validate config (checks for duplicate pipelines, batch_size requirements, etc.)
    config.validate()?;

    let object_store = create_object_store(&config.output_store)?;
    let store = AnalyticsStore::new(object_store.clone(), config.clone(), metrics.clone());

    // Find checkpoint range (snaps to file boundaries in migration mode)
    let (adjusted_first_checkpoint, adjusted_last_checkpoint) = store
        .find_checkpoint_range(indexer_args.first_checkpoint, indexer_args.last_checkpoint)
        .await?;

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

    let rpc_url = client_args
        .ingestion
        .rpc_api_url
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_default();
    let package_cache_path = work_dir.join("package_cache");
    let package_cache = Arc::new(PackageCache::new(&package_cache_path, &rpc_url));

    let adjusted_indexer_args = IndexerArgs {
        first_checkpoint: adjusted_first_checkpoint,
        last_checkpoint: adjusted_last_checkpoint,
        pipeline: indexer_args.pipeline,
        max_rows_per_second: indexer_args.max_rows_per_second,
        task: indexer_args.task,
    };

    let ingestion_config = config.ingestion.clone();

    let mut indexer = Indexer::new(
        store.clone(),
        adjusted_indexer_args,
        client_args,
        ingestion_config,
        None,
        &registry,
    )
    .await?;

    let base_committer = config.committer.clone().finish(CommitterConfig::default());
    let base_sequential = SequentialConfig {
        committer: base_committer,
        ..Default::default()
    };

    for pipeline_config in config.pipeline_configs() {
        info!("Registering pipeline: {}", pipeline_config.pipeline);
        let sequential = pipeline_config
            .sequential
            .clone()
            .finish(base_sequential.clone());
        pipeline_config
            .pipeline
            .register(
                &mut indexer,
                pipeline_config,
                package_cache.clone(),
                metrics.clone(),
                sequential,
            )
            .await?;
    }

    // Spawn Snowflake monitors (if configured)
    let cancel = CancellationToken::new();
    let sf_handles = spawn_snowflake_monitors(&config, metrics, cancel.clone())?;

    // Run the indexer and register shutdown signals
    let service = indexer.run().await?;
    Ok(service
        .with_shutdown_signal(async move {
            store.shutdown().await;
        })
        .with_shutdown_signal(async move {
            cancel.cancel();
            for handle in sf_handles {
                let _ = handle.await;
            }
        }))
}

fn create_object_store(config: &OutputStoreConfig) -> Result<Arc<dyn object_store::ObjectStore>> {
    match config {
        OutputStoreConfig::Gcs {
            bucket,
            service_account_path,
            custom_headers,
            request_timeout_secs,
        } => {
            let mut client_options =
                ClientOptions::default().with_timeout(Duration::from_secs(*request_timeout_secs));

            // Apply custom headers (e.g., for requester-pays buckets)
            if let Some(headers_map) = custom_headers {
                let mut headers = HeaderMap::new();
                for (key, value) in headers_map {
                    headers.insert(
                        HeaderName::try_from(key.as_str())?,
                        HeaderValue::from_str(value)?,
                    );
                }
                client_options = client_options.with_default_headers(headers);
            }

            GoogleCloudStorageBuilder::new()
                .with_client_options(client_options)
                .with_bucket_name(bucket)
                .with_service_account_path(service_account_path.to_string_lossy())
                .build()
                .map(|s| Arc::new(s) as Arc<dyn object_store::ObjectStore>)
                .context("Failed to create GCS store")
        }
        OutputStoreConfig::S3 {
            bucket,
            region,
            access_key_id,
            secret_access_key,
            endpoint,
            request_timeout_secs,
        } => {
            let client_options =
                ClientOptions::default().with_timeout(Duration::from_secs(*request_timeout_secs));
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
            request_timeout_secs,
        } => {
            let client_options =
                ClientOptions::default().with_timeout(Duration::from_secs(*request_timeout_secs));
            MicrosoftAzureBuilder::new()
                .with_client_options(client_options)
                .with_container_name(container)
                .with_account(account)
                .with_access_key(access_key)
                .build()
                .map(|s| Arc::new(s) as Arc<dyn object_store::ObjectStore>)
                .context("Failed to create Azure store")
        }
        OutputStoreConfig::File { path } => LocalFileSystem::new_with_prefix(path)
            .map(|s| Arc::new(s) as Arc<dyn object_store::ObjectStore>)
            .context("Failed to create file store"),
        OutputStoreConfig::Custom(store) => Ok(store.clone()),
    }
}
