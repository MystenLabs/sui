// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, net::SocketAddr, sync::Arc};

use anyhow::{ensure, Context, Result};
use db::{Db, DbArgs};
use diesel_migrations::EmbeddedMigrations;
use ingestion::{client::IngestionClient, ClientArgs, IngestionConfig, IngestionService};
use metrics::{IndexerMetrics, MetricsService};
use pipeline::{
    concurrent::{self, ConcurrentConfig},
    sequential::{self, SequentialConfig},
    Processor,
};
use task::graceful_shutdown;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use watermarks::CommitterWatermark;

pub mod db;
pub mod ingestion;
pub(crate) mod metrics;
pub mod pipeline;
pub(crate) mod schema;
pub mod task;
pub(crate) mod watermarks;

/// Command-line arguments for the indexer
#[derive(clap::Args, Debug, Clone)]
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

    /// Address to serve Prometheus Metrics from.
    #[arg(long, default_value_t = Self::default().metrics_address)]
    pub metrics_address: SocketAddr,
}

pub struct Indexer {
    /// Connection pool to the database.
    db: Db,

    /// Prometheus Metrics.
    metrics: Arc<IndexerMetrics>,

    /// Service for serving Prometheis metrics.
    metrics_service: MetricsService,

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

impl Indexer {
    /// Create a new instance of the indexer framework. `db_args`, `indexer_args,`, `client_args`,
    /// and `ingestion_config` contain configurations for the following, respectively:
    ///
    /// - Connecting to the database,
    /// - What is indexed (which checkpoints, which pipelines, whether to update the watermarks
    ///   table) and where to serve metrics from,
    /// - Where to download checkpoints from,
    /// - Concurrency and buffering parameters for downloading checkpoints.
    ///
    /// `migrations` contains the SQL to run in order to bring the database schema up-to-date for
    /// the specific instance of the indexer, generated using diesel's `embed_migrations!` macro.
    /// These migrations will be run as part of initializing the indexer.
    ///
    /// After initialization, at least one pipeline must be added using [Self::concurrent_pipeline]
    /// or [Self::sequential_pipeline], before the indexer is started using [Self::run].
    pub async fn new(
        db_args: DbArgs,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        ingestion_config: IngestionConfig,
        migrations: &'static EmbeddedMigrations,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let IndexerArgs {
            first_checkpoint,
            last_checkpoint,
            pipeline,
            skip_watermark,
            metrics_address,
        } = indexer_args;

        let db = Db::new(db_args)
            .await
            .context("Failed to connect to database")?;

        // At indexer initialization, we ensure that the DB schema is up-to-date.
        db.run_migrations(migrations)
            .await
            .context("Failed to run pending migrations")?;

        let (metrics, metrics_service) =
            MetricsService::new(metrics_address, db.clone(), cancel.clone())?;

        let ingestion_service = IngestionService::new(
            client_args,
            ingestion_config,
            metrics.clone(),
            cancel.clone(),
        )?;

        Ok(Self {
            db,
            metrics,
            metrics_service,
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

    /// The database connection pool used by the indexer.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// The ingestion client used by the indexer to fetch checkpoints.
    pub fn ingestion_client(&self) -> &IngestionClient {
        self.ingestion_service.client()
    }

    /// Adds a new pipeline to this indexer and starts it up. Although their tasks have started,
    /// they will be idle until the ingestion service starts, and serves it checkpoint data.
    ///
    /// Concurrent pipelines commit checkpoint data out-of-order to maximise throughput, and they
    /// keep the watermark table up-to-date with the highest point they can guarantee all data
    /// exists for, for their pipeline.
    pub async fn concurrent_pipeline<H: concurrent::Handler + Send + Sync + 'static>(
        &mut self,
        handler: H,
        config: ConcurrentConfig,
    ) -> Result<()> {
        let Some(watermark) = self.add_pipeline::<H>().await? else {
            return Ok(());
        };

        // For a concurrent pipeline, if skip_watermark is set, we don't really care about the
        // watermark consistency. first_checkpoint can be anything since we don't update watermark,
        // and writes should be idempotent.
        if !self.skip_watermark {
            self.check_first_checkpoint_consistency::<H>(&watermark)?;
        }

        self.handles.push(concurrent::pipeline(
            handler,
            watermark,
            config,
            self.skip_watermark,
            self.db.clone(),
            self.ingestion_service.subscribe().0,
            self.metrics.clone(),
            self.cancel.clone(),
        ));

        Ok(())
    }

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
    pub async fn sequential_pipeline<H: sequential::Handler + Send + Sync + 'static>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> Result<()> {
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

        self.handles.push(sequential::pipeline(
            handler,
            watermark,
            config,
            self.db.clone(),
            checkpoint_rx,
            watermark_tx,
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
                first_checkpoint as i64 <= watermark.checkpoint_hi_inclusive + 1,
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

        let metrics_handle = self
            .metrics_service
            .run()
            .await
            .context("Failed to start metrics service")?;

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

        let cancel = self.cancel.clone();
        Ok(tokio::spawn(async move {
            // Wait for the ingestion service and all its related tasks to wind down gracefully:
            // If ingestion has been configured to only handle a specific range of checkpoints, we
            // want to make sure that tasks are allowed to run to completion before shutting them
            // down.
            graceful_shutdown(self.handles, self.cancel).await;

            info!("Indexing pipeline gracefully shut down");

            // Pick off any stragglers (in this case, just the metrics service).
            cancel.cancel();
            metrics_handle.await.unwrap();
        }))
    }

    /// Update the indexer's first checkpoint based on the watermark for the pipeline by adding for
    /// handler `H` (as long as it's enabled). Returns `Ok(None)` if the pipeline is disabled,
    /// `Ok(Some(None))` if the pipeline is enabled but its watermark is not found, and
    /// `Ok(Some(Some(watermark)))` if the pipeline is enabled and the watermark is found.
    async fn add_pipeline<P: Processor + 'static>(
        &mut self,
    ) -> Result<Option<Option<CommitterWatermark<'static>>>> {
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

        let mut conn = self.db.connect().await.context("Failed DB connection")?;

        let watermark = CommitterWatermark::get(&mut conn, P::NAME)
            .await
            .with_context(|| format!("Failed to get watermark for {}", P::NAME))?;

        // TODO(amnn): Test this (depends on supporting migrations and tempdb).
        self.first_checkpoint_from_watermark = watermark
            .as_ref()
            .map_or(0, |w| w.checkpoint_hi_inclusive as u64 + 1)
            .min(self.first_checkpoint_from_watermark);

        Ok(Some(watermark))
    }
}

impl Default for IndexerArgs {
    fn default() -> Self {
        Self {
            first_checkpoint: None,
            last_checkpoint: None,
            pipeline: vec![],
            skip_watermark: false,
            metrics_address: "0.0.0.0:9184".parse().unwrap(),
        }
    }
}
