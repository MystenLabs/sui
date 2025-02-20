// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use anyhow::{ensure, Context, Result};
use diesel::{
    migration::{self, Migration, MigrationSource},
    pg::Pg,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations};
use ingestion::{client::IngestionClient, ClientArgs, IngestionConfig, IngestionService};
use metrics::IndexerMetrics;
use models::watermarks::{CommitterWatermark, PrunerWatermark};
use pipeline::{
    concurrent::{self, ConcurrentConfig},
    sequential::{self, SequentialConfig},
    Processor,
};
use prometheus::Registry;
use sui_indexer_alt_metrics::db::DbConnectionStatsCollector;
use sui_pg_db::{temp::TempDb, Db, DbArgs};
use task::graceful_shutdown;
use tempfile::tempdir;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

pub mod handlers;
pub mod ingestion;
pub(crate) mod metrics;
pub mod models;
pub mod pipeline;
pub mod schema;
pub mod task;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

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

pub struct Indexer {
    /// Connection pool to the database.
    db: Db,

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
    /// Optional `migrations` contains the SQL to run in order to bring the database schema up-to-date for
    /// the specific instance of the indexer, generated using diesel's `embed_migrations!` macro.
    /// These migrations will be run as part of initializing the indexer if provided.
    ///
    /// After initialization, at least one pipeline must be added using [Self::concurrent_pipeline]
    /// or [Self::sequential_pipeline], before the indexer is started using [Self::run].
    pub async fn new(
        db_args: DbArgs,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        ingestion_config: IngestionConfig,
        migrations: Option<&'static EmbeddedMigrations>,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let IndexerArgs {
            first_checkpoint,
            last_checkpoint,
            pipeline,
            skip_watermark,
        } = indexer_args;

        let db = Db::for_write(db_args)
            .await
            .context("Failed to connect to database")?;

        // At indexer initialization, we ensure that the DB schema is up-to-date.
        db.run_migrations(Self::migrations(migrations))
            .await
            .context("Failed to run pending migrations")?;

        let metrics = IndexerMetrics::new(registry);
        registry.register(Box::new(DbConnectionStatsCollector::new(
            Some("indexer_db"),
            db.clone(),
        )))?;

        let ingestion_service = IngestionService::new(
            client_args,
            ingestion_config,
            metrics.clone(),
            cancel.clone(),
        )?;

        Ok(Self {
            db,
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

    pub async fn new_for_testing(migrations: &'static EmbeddedMigrations) -> (Self, TempDb) {
        let temp_db = TempDb::new().unwrap();
        let db_args = DbArgs::new_for_testing(temp_db.database().url().clone());
        let indexer = Indexer::new(
            db_args,
            IndexerArgs::default(),
            ClientArgs {
                remote_store_url: None,
                local_ingestion_path: Some(tempdir().unwrap().into_path()),
            },
            IngestionConfig::default(),
            Some(migrations),
            &Registry::new(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        (indexer, temp_db)
    }

    /// The database connection pool used by the indexer.
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// The ingestion client used by the indexer to fetch checkpoints.
    pub fn ingestion_client(&self) -> &IngestionClient {
        self.ingestion_service.client()
    }

    /// The pipelines that this indexer will run.
    pub fn pipelines(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.added_pipelines.iter().copied().filter(|p| {
            self.enabled_pipelines
                .as_ref()
                .map_or(true, |e| e.contains(*p))
        })
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
        let start_from_pruner_watermark = H::PRUNING_REQUIRES_PROCESSED_VALUES;
        let Some(watermark) = self.add_pipeline::<H>(start_from_pruner_watermark).await? else {
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
        let Some(watermark) = self.add_pipeline::<H>(false).await? else {
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
            graceful_shutdown(self.handles, self.cancel).await;
            info!("Indexing pipeline gracefully shut down");
        }))
    }

    /// Combine the provided `migrations` with the migrations necessary to set up the indexer
    /// framework. The returned migration source can be passed to [Db::run_migrations] to ensure
    /// the database's schema is up-to-date for both the indexer framework and the specific
    /// indexer.
    pub fn migrations(
        migrations: Option<&'static EmbeddedMigrations>,
    ) -> impl MigrationSource<Pg> + Send + Sync + 'static {
        struct Migrations(Option<&'static EmbeddedMigrations>);
        impl MigrationSource<Pg> for Migrations {
            fn migrations(&self) -> migration::Result<Vec<Box<dyn Migration<Pg>>>> {
                let mut migrations = MIGRATIONS.migrations()?;
                if let Some(more_migrations) = self.0 {
                    migrations.extend(more_migrations.migrations()?);
                }
                Ok(migrations)
            }
        }

        Migrations(migrations)
    }

    /// Update the indexer's first checkpoint based on the watermark for the pipeline by adding for
    /// handler `H` (as long as it's enabled). Returns `Ok(None)` if the pipeline is disabled,
    /// `Ok(Some(None))` if the pipeline is enabled but its watermark is not found, and
    /// `Ok(Some(Some(watermark)))` if the pipeline is enabled and the watermark is found.
    ///
    /// If `start_from_pruner_watermark` is true, the indexer will start ingestion from just after
    /// the pruner watermark, so that the pruner have access to the processed values for any
    /// unpruned checkpoints.
    async fn add_pipeline<P: Processor + 'static>(
        &mut self,
        start_from_pruner_watermark: bool,
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

        let expected_first_checkpoint = if start_from_pruner_watermark {
            // If the pruner of this pipeline requires processed values in order to prune,
            // we must start ingestion from just after the pruner watermark,
            // so that we can process all values needed by the pruner.
            PrunerWatermark::get(&mut conn, P::NAME, Default::default())
                .await
                .with_context(|| format!("Failed to get pruner watermark for {}", P::NAME))?
                .map(|w| w.pruner_hi as u64)
                .unwrap_or_default()
        } else {
            watermark
                .as_ref()
                .map(|w| w.checkpoint_hi_inclusive as u64 + 1)
                .unwrap_or_default()
        };

        self.first_checkpoint_from_watermark =
            expected_first_checkpoint.min(self.first_checkpoint_from_watermark);

        Ok(Some(watermark))
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use sui_field_count::FieldCount;
    use sui_pg_db as db;
    use sui_types::full_checkpoint_content::CheckpointData;

    use super::*;

    #[derive(FieldCount)]
    struct V {
        _v: u64,
    }

    macro_rules! define_test_concurrent_pipeline {
        ($name:ident) => {
            define_test_concurrent_pipeline!($name, false);
        };
        ($name:ident, $pruning_requires_processed_values:expr) => {
            struct $name;
            impl Processor for $name {
                const NAME: &'static str = stringify!($name);
                type Value = V;
                fn process(
                    &self,
                    _checkpoint: &Arc<CheckpointData>,
                ) -> anyhow::Result<Vec<Self::Value>> {
                    todo!()
                }
            }

            #[async_trait]
            impl concurrent::Handler for $name {
                const PRUNING_REQUIRES_PROCESSED_VALUES: bool = $pruning_requires_processed_values;
                async fn commit(
                    _values: &[Self::Value],
                    _conn: &mut db::Connection<'_>,
                ) -> anyhow::Result<usize> {
                    todo!()
                }
            }
        };
    }

    define_test_concurrent_pipeline!(ConcurrentPipeline1);
    define_test_concurrent_pipeline!(ConcurrentPipeline2);
    define_test_concurrent_pipeline!(ConcurrentPipeline3, true);

    #[tokio::test]
    async fn test_add_new_pipeline() {
        let (mut indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 0);
    }

    #[tokio::test]
    async fn test_add_existing_pipeline() {
        let (mut indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let watermark = CommitterWatermark::new_for_testing(ConcurrentPipeline1::NAME, 10);
        watermark
            .update(&mut indexer.db().connect().await.unwrap())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 11);
    }

    #[tokio::test]
    async fn test_add_multiple_pipelines() {
        let (mut indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let watermark1 = CommitterWatermark::new_for_testing(ConcurrentPipeline1::NAME, 10);
        watermark1
            .update(&mut indexer.db().connect().await.unwrap())
            .await
            .unwrap();
        let watermark2 = CommitterWatermark::new_for_testing(ConcurrentPipeline2::NAME, 20);
        watermark2
            .update(&mut indexer.db().connect().await.unwrap())
            .await
            .unwrap();

        indexer
            .concurrent_pipeline(ConcurrentPipeline2, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 21);
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 11);
    }

    #[tokio::test]
    async fn test_add_multiple_pipelines_pruning_requires_processed_values() {
        let (mut indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let watermark1 = CommitterWatermark::new_for_testing(ConcurrentPipeline1::NAME, 10);
        watermark1
            .update(&mut indexer.db().connect().await.unwrap())
            .await
            .unwrap();
        indexer
            .concurrent_pipeline(ConcurrentPipeline1, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 11);

        let watermark3 = CommitterWatermark::new_for_testing(ConcurrentPipeline3::NAME, 20);
        watermark3
            .update(&mut indexer.db().connect().await.unwrap())
            .await
            .unwrap();
        let pruner_watermark = PrunerWatermark::new_for_testing(ConcurrentPipeline3::NAME, 5);
        assert!(pruner_watermark
            .update(&mut indexer.db().connect().await.unwrap())
            .await
            .unwrap());
        indexer
            .concurrent_pipeline(ConcurrentPipeline3, ConcurrentConfig::default())
            .await
            .unwrap();
        assert_eq!(indexer.first_checkpoint_from_watermark, 5);
    }
}
