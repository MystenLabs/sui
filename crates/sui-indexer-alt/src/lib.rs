// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use db::{Db, DbConfig};
use handlers::{pipeline, CommitterConfig, Handler};
use ingestion::{IngestionConfig, IngestionService};
use metrics::{IndexerMetrics, MetricsService};
use task::graceful_shutdown;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub mod args;
pub mod db;
pub mod handlers;
pub mod ingestion;
pub mod metrics;
pub mod models;
pub mod schema;
pub mod task;

pub struct Indexer {
    db: Db,
    metrics: Arc<IndexerMetrics>,
    metrics_service: MetricsService,
    ingestion_service: IngestionService,
    committer_config: CommitterConfig,
    cancel: CancellationToken,
    handles: Vec<JoinHandle<()>>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct IndexerConfig {
    #[command(flatten)]
    pub ingestion_config: IngestionConfig,

    #[command(flatten)]
    pub db_config: DbConfig,

    #[command(flatten)]
    pub committer_config: CommitterConfig,

    /// Address to serve Prometheus Metrics from.
    #[arg(long, default_value = "0.0.0.0:9184")]
    pub metrics_address: SocketAddr,
}

impl Indexer {
    pub async fn new(config: IndexerConfig, cancel: CancellationToken) -> Result<Self> {
        let IndexerConfig {
            ingestion_config,
            db_config,
            committer_config,
            metrics_address,
        } = config;

        let db = Db::new(db_config)
            .await
            .context("Failed to connect to database")?;

        let (metrics, metrics_service) =
            MetricsService::new(metrics_address, db.clone(), cancel.clone())?;
        let ingestion_service =
            IngestionService::new(ingestion_config, metrics.clone(), cancel.clone())?;

        Ok(Self {
            db,
            metrics,
            metrics_service,
            ingestion_service,
            committer_config,
            cancel,
            handles: vec![],
        })
    }

    pub fn pipeline<H: Handler + 'static>(&mut self) {
        let (handler, committer) = pipeline::<H>(
            self.db.clone(),
            self.ingestion_service.subscribe(),
            self.committer_config.clone(),
            self.metrics.clone(),
            self.cancel.clone(),
        );

        self.handles.push(handler);
        self.handles.push(committer);
    }

    pub async fn run(mut self) -> Result<JoinHandle<()>> {
        self.handles.push(
            self.metrics_service
                .run()
                .await
                .context("Failed to start metrics service")?,
        );

        self.handles.push(
            self.ingestion_service
                .run()
                .await
                .context("Failed to start ingestion service")?,
        );

        Ok(tokio::spawn(graceful_shutdown(self.handles, self.cancel)))
    }
}
