// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::handlers::ev_emit_mod::EvEmitMod;
use crate::handlers::ev_struct_inst::EvStructInst;
use crate::handlers::kv_checkpoints::KvCheckpoints;
use crate::handlers::kv_objects::KvObjects;
use crate::handlers::kv_transactions::KvTransactions;
use crate::handlers::tx_affected_objects::TxAffectedObjects;
use crate::handlers::tx_balance_changes::TxBalanceChanges;
use crate::pipeline::PipelineName;
use anyhow::{Context, Result};
use db::{Db, DbConfig};
use handlers::Handler;
use ingestion::{IngestionConfig, IngestionService};
use metrics::{IndexerMetrics, MetricsService};
use models::watermarks::CommitterWatermark;
use pipeline::{concurrent, PipelineConfig};
use std::{net::SocketAddr, sync::Arc};
use task::graceful_shutdown;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::info;

pub mod args;
pub mod db;
pub mod handlers;
pub mod ingestion;
pub mod metrics;
pub mod models;
pub mod pipeline;
pub mod schema;
pub mod task;

pub struct Indexer {
    /// Connection pool to the database.
    db: Db,

    /// Prometheus Metrics.
    metrics: Arc<IndexerMetrics>,

    /// Service for serving Prometheis metrics.
    metrics_service: MetricsService,

    /// Service for downloading and disseminating checkpoint data.
    ingestion_service: IngestionService,

    /// Parameters for the committers of each pipeline.
    pipeline_config: PipelineConfig,

    /// Optional override of the checkpoint lowerbound.
    first_checkpoint: Option<u64>,

    /// Optional override of the checkpoint upperbound.
    last_checkpoint: Option<u64>,

    /// Cancellation token shared among all continuous tasks in the service.
    cancel: CancellationToken,

    /// The checkpoint lowerbound derived from watermarks of pipelines added to the indexer. When
    /// the indexer runs, it will start from this point, unless this has been overridden by
    /// [Self::first_checkpoint].
    first_checkpoint_from_watermark: u64,

    /// The handles for every task spawned by this indexer, used to manage graceful shutdown.
    handles: Vec<JoinHandle<()>>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct IndexerConfig {
    #[command(flatten)]
    pub ingestion_config: IngestionConfig,

    #[command(flatten)]
    pub db_config: DbConfig,

    #[command(flatten)]
    pub pipeline_config: PipelineConfig,

    /// Override for the checkpoint to start ingestion from -- useful for backfills. By default,
    /// ingestion will start just after the lowest checkpoint watermark across all active
    /// pipelines.
    #[arg(long)]
    first_checkpoint: Option<u64>,

    /// Override for the checkpoint to end ingestion at (inclusive) -- useful for backfills. By
    /// default, ingestion will not stop, and will continue to poll for new checkpoints.
    #[arg(long)]
    last_checkpoint: Option<u64>,

    /// Address to serve Prometheus Metrics from.
    #[arg(long, default_value = "0.0.0.0:9184")]
    pub metrics_address: SocketAddr,
}

impl Indexer {
    pub async fn new(config: IndexerConfig, cancel: CancellationToken) -> Result<Self> {
        let IndexerConfig {
            ingestion_config,
            db_config,
            pipeline_config,
            first_checkpoint,
            last_checkpoint,
            metrics_address,
        } = config;

        let db = Db::new(db_config)
            .await
            .context("Failed to connect to database")?;

        let (metrics, metrics_service) =
            MetricsService::new(metrics_address, db.clone(), cancel.clone())?;
        let ingestion_service =
            IngestionService::new(ingestion_config, metrics.clone(), cancel.clone())?;

        let mut indexer = Self {
            db,
            metrics,
            metrics_service,
            ingestion_service,
            pipeline_config,
            first_checkpoint,
            last_checkpoint,
            cancel,
            first_checkpoint_from_watermark: u64::MAX,
            handles: vec![],
        };
        indexer.register_pipeline().await?;
        Ok(indexer)
    }

    async fn register_pipeline(&mut self) -> Result<()> {
        match self.pipeline_config.pipeline {
            PipelineName::EvEmitMod => self.concurrent_pipeline::<EvEmitMod>().await?,
            PipelineName::EvStructInst => self.concurrent_pipeline::<EvStructInst>().await?,
            PipelineName::KvCheckpoints => self.concurrent_pipeline::<KvCheckpoints>().await?,
            PipelineName::KvObjects => self.concurrent_pipeline::<KvObjects>().await?,
            PipelineName::KvTransactions => self.concurrent_pipeline::<KvTransactions>().await?,
            PipelineName::TxAffectedObjects => {
                self.concurrent_pipeline::<TxAffectedObjects>().await?
            }
            PipelineName::TxBalanceChanges => {
                self.concurrent_pipeline::<TxBalanceChanges>().await?
            }
        }

        Ok(())
    }

    /// Adds a new pipeline to this indexer and starts it up. Although their tasks have started,
    /// they will be idle until the ingestion service starts, and serves it checkpoint data.
    pub async fn concurrent_pipeline<H: Handler + 'static>(&mut self) -> Result<()> {
        let mut conn = self.db.connect().await.context("Failed DB connection")?;

        let watermark = CommitterWatermark::get(&mut conn, H::NAME)
            .await
            .with_context(|| format!("Failed to get watermark for {}", H::NAME))?;

        // TODO(amnn): Test this (depends on supporting migrations and tempdb).
        self.first_checkpoint_from_watermark = watermark
            .as_ref()
            .map_or(0, |w| w.checkpoint_hi_inclusive as u64 + 1)
            .min(self.first_checkpoint_from_watermark);

        let (processor, committer, watermark) = concurrent::pipeline::<H>(
            watermark,
            self.pipeline_config.clone(),
            self.db.clone(),
            self.ingestion_service.subscribe().0,
            self.metrics.clone(),
            self.cancel.clone(),
        );

        self.handles.push(processor);
        self.handles.push(committer);
        self.handles.push(watermark);

        Ok(())
    }

    /// Start ingesting checkpoints. Ingestion either starts from the configured
    /// `first_checkpoint`, or it is calculated based on the watermarks of all active pipelines.
    /// Ingestion will stop after consuming the configured `last_checkpoint`, if one is provided,
    /// or will continue until it tracks the tip of the network.
    pub async fn run(mut self) -> Result<JoinHandle<()>> {
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
}
