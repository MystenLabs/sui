// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{ensure, Context};
use futures::future;
use ingestion::{client::IngestionClient, ClientArgs, IngestionConfig, IngestionService};
use metrics::IndexerMetrics;
use pipeline::{
    concurrent::{self, ConcurrentConfig},
    sequential::{self, Handler, SequentialConfig},
    Processor,
};
use prometheus::Registry;
use sui_indexer_alt_framework_store_traits::{
    CommitterWatermark, Connection, Store, TransactionalStore,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub use anyhow::Result;
pub use sui_field_count::FieldCount;
/// External users access the store trait through framework::store
pub use sui_indexer_alt_framework_store_traits as store;
pub use sui_types as types;

#[cfg(feature = "cluster")]
pub mod cluster;
pub mod ingestion;
pub mod metrics;
pub mod pipeline;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod task;

/// Command-line arguments for the indexer
#[derive(clap::Args, Default, Debug, Clone)]
pub struct IndexerArgs {
    /// Override for the checkpoint to start ingestion from -- useful for backfills. By default,
    /// ingestion will start just after the lowest checkpoint watermark across all active
    /// pipelines.
    #[arg(long)]
    pub first_checkpoint: Option<u64>,

    /// Override for the checkpoint to end ingestion at (inclusive) -- useful for backfills. By
    /// default, ingestion will not stop, and will continue to poll for new checkpoints.
    #[arg(long)]
    pub last_checkpoint: Option<u64>,

    /// Only run the following pipelines. If not provided, all pipelines found in the
    /// configuration file will be run.
    #[arg(long, action = clap::ArgAction::Append)]
    pub pipeline: Vec<String>,

    /// Don't write to the watermark tables for concurrent pipelines.
    #[arg(long)]
    pub skip_watermark: bool,
}

pub struct Indexer<S: Store> {
    /// The storage backend that the indexer uses to write and query indexed data. This
    /// generic implementation allows for plugging in different storage solutions that implement the
    /// `Store` trait.
    store: S,

    /// Prometheus Metrics.
    metrics: Arc<IndexerMetrics>,

    /// Service for downloading and disseminating checkpoint data.
    ingestion_service: IngestionService,

    /// Optional override of the checkpoint lowerbound.
    first_checkpoint: Option<u64>,

    /// Optional override of the checkpoint upperbound.
    last_checkpoint: Option<u64>,

    /// Don't write to the watermark tables for concurrent pipelines.
    skip_watermark: bool,

    /// Optional filter for pipelines to run. If `None`, all pipelines added to the indexer will
    /// run. Any pipelines that are present in this filter but not added to the indexer will yield
    /// a warning when the indexer is run.
    enabled_pipelines: Option<BTreeSet<String>>,

    /// Pipelines that have already been registered with the indexer. Used to make sure a pipeline
    /// with the same name isn't added twice.
    added_pipelines: BTreeSet<&'static str>,

    /// Cancellation token shared among all continuous tasks in the service.
    cancel: CancellationToken,

    /// The checkpoint lowerbound derived from watermarks of pipelines added to the indexer. When
    /// the indexer runs, it will start from this point, unless this has been overridden by
    /// [Self::first_checkpoint].
    first_checkpoint_from_watermark: u64,

    /// The handles for every task spawned by this indexer, used to manage graceful shutdown.
    handles: Vec<JoinHandle<()>>,
}

impl<S: Store> Indexer<S> {
    /// Create a new instance of the indexer framework from a store that implements the `Store`
    /// trait, along with `indexer_args`, `client_args`, and `ingestion_config`. Together, these
    /// arguments configure the following:
    ///
    /// - What is indexed (which checkpoints, which pipelines, whether to track and update
    ///   watermarks) and where to serve metrics from,
    /// - Where to download checkpoints from,
    /// - Concurrency and buffering parameters for downloading checkpoints.
    ///
    /// After initialization, at least one pipeline must be added using [Self::concurrent_pipeline]
    /// or [Self::sequential_pipeline], before the indexer is started using [Self::run].
    pub async fn new(
        store: S,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        ingestion_config: IngestionConfig,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let IndexerArgs {
            first_checkpoint,
            last_checkpoint,
            pipeline,
            skip_watermark,
        } = indexer_args;

        let metrics = IndexerMetrics::new(registry);

        let ingestion_service = IngestionService::new(
            client_args,
            ingestion_config,
            metrics.clone(),
            cancel.clone(),
        )?;

        Ok(Self {
            store,
            metrics,
            ingestion_service,
            first_checkpoint,
            last_checkpoint,
            skip_watermark,
            enabled_pipelines: if pipeline.is_empty() {
                None
            } else {
                Some(pipeline.into_iter().collect())
            },
            added_pipelines: BTreeSet::new(),
            cancel,
            first_checkpoint_from_watermark: u64::MAX,
            handles: vec![],
        })
    }

    /// The store used by the indexer.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The ingestion client used by the indexer to fetch checkpoints.
    pub fn ingestion_client(&self) -> &IngestionClient {
        self.ingestion_service.client()
    }

    /// The indexer's metrics.
    pub fn metrics(&self) -> &Arc<IndexerMetrics> {
        &self.metrics
    }

    /// The pipelines that this indexer will run.
    pub fn pipelines(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.added_pipelines.iter().copied().filter(|p| {
            self.enabled_pipelines
                .as_ref()
                .is_none_or(|e| e.contains(*p))
        })
    }

    /// Adds a new pipeline to this indexer and starts it up. Although their tasks have started,
    /// they will be idle until the ingestion service starts, and serves it checkpoint data.
    ///
    /// Concurrent pipelines commit checkpoint data out-of-order to maximise throughput, and they
    /// keep the watermark table up-to-date with the highest point they can guarantee all data
    /// exists for, for their pipeline.
    pub async fn concurrent_pipeline<H>(
        &mut self,
        handler: H,
        config: ConcurrentConfig,
    ) -> Result<()>
    where
        H: concurrent::Handler<Store = S> + Send + Sync + 'static,
    {
        let Some(watermark) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        // For a concurrent pipeline, if skip_watermark is set, we don't really care about the
        // watermark consistency. first_checkpoint can be anything since we don't update watermark,
        // and writes should be idempotent.
        if !self.skip_watermark {
            self.check_first_checkpoint_consistency::<H>(&watermark)?;
        }

        self.handles.push(concurrent::pipeline::<H>(
            handler,
            watermark,
            config,
            self.skip_watermark,
            self.store.clone(),
            self.ingestion_service.subscribe().0,
            self.metrics.clone(),
            self.cancel.clone(),
        ));

        Ok(())
    }

    /// Checks that the first checkpoint override is consistent with the watermark for the pipeline.
    /// If the watermark does not exist, the override can be anything. If the watermark exists, the
    /// override must not leave any gap in the data: it can be in the past, or at the tip of the
    /// network, but not in the future.
    fn check_first_checkpoint_consistency<P: Processor>(
        &self,
        watermark: &Option<CommitterWatermark>,
    ) -> Result<()> {
        if let (Some(watermark), Some(first_checkpoint)) = (watermark, self.first_checkpoint) {
            ensure!(
                first_checkpoint <= watermark.checkpoint_hi_inclusive + 1,
                "For pipeline {}, first checkpoint override {} is too far ahead of watermark {}. \
                 This could create gaps in the data.",
                P::NAME,
                first_checkpoint,
                watermark.checkpoint_hi_inclusive,
            );
        }

        Ok(())
    }

    /// Start ingesting checkpoints. Ingestion either starts from the configured
    /// `first_checkpoint`, or it is calculated based on the watermarks of all active pipelines.
    /// Ingestion will stop after consuming the configured `last_checkpoint`, if one is provided,
    /// or will continue until it tracks the tip of the network.
    pub async fn run(mut self) -> Result<JoinHandle<()>> {
        if let Some(enabled_pipelines) = self.enabled_pipelines {
            ensure!(
                enabled_pipelines.is_empty(),
                "Tried to enable pipelines that this indexer does not know about: \
                {enabled_pipelines:#?}",
            );
        }

        // If an override has been provided, start ingestion from there, otherwise start ingestion
        // from just after the lowest committer watermark across all enabled pipelines.
        let first_checkpoint = self
            .first_checkpoint
            .unwrap_or(self.first_checkpoint_from_watermark);

        let last_checkpoint = self.last_checkpoint.unwrap_or(u64::MAX);

        info!(first_checkpoint, last_checkpoint = ?self.last_checkpoint, "Ingestion range");

        let (regulator_handle, broadcaster_handle) = self
            .ingestion_service
            .run(first_checkpoint..=last_checkpoint)
            .await
            .context("Failed to start ingestion service")?;

        self.handles.push(regulator_handle);
        self.handles.push(broadcaster_handle);

        Ok(tokio::spawn(async move {
            // Wait for the ingestion service and all its related tasks to wind down gracefully:
            // If ingestion has been configured to only handle a specific range of checkpoints, we
            // want to make sure that tasks are allowed to run to completion before shutting them
            // down.
            future::join_all(self.handles).await;
            info!("Indexing pipeline gracefully shut down");
        }))
    }

    /// Update the indexer's first checkpoint based on the watermark for the pipeline by adding for
    /// handler `H` (as long as it's enabled). Returns `Ok(None)` if the pipeline is disabled,
    /// `Ok(Some(None))` if the pipeline is enabled but its watermark is not found, and
    /// `Ok(Some(Some(watermark)))` if the pipeline is enabled and the watermark is found.
    async fn add_pipeline<P: Processor + 'static>(
        &mut self,
    ) -> Result<Option<Option<CommitterWatermark>>> {
        ensure!(
            self.added_pipelines.insert(P::NAME),
            "Pipeline {:?} already added",
            P::NAME,
        );

        if let Some(enabled_pipelines) = &mut self.enabled_pipelines {
            if !enabled_pipelines.remove(P::NAME) {
                info!(pipeline = P::NAME, "Skipping");
                return Ok(None);
            }
        }

        let mut conn = self
            .store
            .connect()
            .await
            .context("Failed to establish connection to store")?;

        let watermark = conn
            .committer_watermark(P::NAME)
            .await
            .with_context(|| format!("Failed to get watermark for {}", P::NAME))?;

        let expected_first_checkpoint = watermark
            .as_ref()
            .map(|w| w.checkpoint_hi_inclusive + 1)
            .unwrap_or_default();

        self.first_checkpoint_from_watermark =
            expected_first_checkpoint.min(self.first_checkpoint_from_watermark);

        Ok(Some(watermark))
    }
}

impl<T: TransactionalStore> Indexer<T> {
    /// Adds a new pipeline to this indexer and starts it up. Although their tasks have started,
    /// they will be idle until the ingestion service starts, and serves it checkpoint data.
    ///
    /// Sequential pipelines commit checkpoint data in-order which sacrifices throughput, but may
    /// be required to handle pipelines that modify data in-place (where each update is not an
    /// insert, but could be a modification of an existing row, where ordering between updates is
    /// important).
    ///
    /// The pipeline can optionally be configured to lag behind the ingestion service by a fixed
    /// number of checkpoints (configured by `checkpoint_lag`).
    pub async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> Result<()>
    where
        H: Handler<Store = T> + Send + Sync + 'static,
    {
        let Some(watermark) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        if self.skip_watermark {
            warn!(
                pipeline = H::NAME,
                "--skip-watermarks enabled and ignored for sequential pipeline"
            );
        }

        // For a sequential pipeline, data must be written in the order of checkpoints.
        // Hence, we do not allow the first_checkpoint override to be in arbitrary positions.
        self.check_first_checkpoint_consistency::<H>(&watermark)?;

        let (checkpoint_rx, watermark_tx) = self.ingestion_service.subscribe();

        self.handles.push(sequential::pipeline::<H>(
            handler,
            watermark,
            config,
            self.store.clone(),
            checkpoint_rx,
            watermark_tx,
            self.metrics.clone(),
            self.cancel.clone(),
        ));

        Ok(())
    }
}

#[cfg(test)]
pub mod testing;
