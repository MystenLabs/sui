// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Analytics indexer builder.

use std::sync::Arc;

use crate::package_store::PackageCache;
use anyhow::{Context, Result};
use tracing::info;

use sui_config::object_storage_config::ObjectStoreConfig;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_object_store::ObjectStore;

use crate::config::{JobConfig, PipelineConfig};

/// Builds and configures an analytics indexer from the given configuration.
pub async fn build_analytics_indexer(
    config: JobConfig,
    registry: prometheus::Registry,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<Indexer<ObjectStore>> {
    let object_store = create_object_store_from_config(&config.remote_store_config).await?;

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

    // Create package cache for handlers that need it
    let package_cache_path = work_dir.join("package_cache");
    let package_cache = Arc::new(PackageCache::new(&package_cache_path, &config.rest_url));

    // Create the indexer args from config
    let indexer_args = sui_indexer_alt_framework::IndexerArgs {
        first_checkpoint: config.first_checkpoint,
        last_checkpoint: config.last_checkpoint,
        pipeline: vec![],
        task: Default::default(),
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

    // Use framework config types directly from JobConfig
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

    // Register pipelines for each enabled file type
    for pipeline_config in config.pipeline_configs() {
        info!("Registering pipeline: {}", pipeline_config.pipeline);

        register_pipeline(
            &mut indexer,
            pipeline_config,
            Some(package_cache.clone()),
            concurrent_config.clone(),
        )
        .await?;
    }

    Ok(indexer)
}

async fn register_pipeline(
    indexer: &mut Indexer<ObjectStore>,
    pipeline_config: &PipelineConfig,
    package_cache: Option<Arc<PackageCache>>,
    config: ConcurrentConfig,
) -> Result<()> {
    pipeline_config
        .pipeline
        .register_handler(indexer, pipeline_config, package_cache, config)
        .await
}

async fn create_object_store_from_config(
    config: &ObjectStoreConfig,
) -> Result<Arc<dyn object_store::ObjectStore>> {
    let store = config
        .make()
        .context("Failed to create object store from configuration")?;
    Ok(store)
}
