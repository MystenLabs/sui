// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use anyhow::Context;
use diesel_migrations::EmbeddedMigrations;
use prometheus::Registry;
use sui_indexer_alt_metrics::{MetricsArgs, MetricsService};
use tokio::{signal, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;
use url::Url;

use crate::postgres::{Db, DbArgs};
use crate::{
    ingestion::{ClientArgs, IngestionConfig},
    Indexer, IndexerArgs, IndexerMetrics, Result,
};

/// Bundle of arguments for setting up an indexer cluster (an Indexer and its associated Metrics
/// service). This struct is offered as a convenience for the common case of parsing command-line
/// arguments for a binary running a standalone indexer and its metrics service.
#[derive(clap::Parser, Debug, Default)]
pub struct Args {
    /// What to index and in what time range.
    #[clap(flatten)]
    pub indexer_args: IndexerArgs,

    /// Where to get checkpoint data from.
    #[clap(flatten)]
    pub client_args: Option<ClientArgs>,

    /// How to expose metrics.
    #[clap(flatten)]
    pub metrics_args: MetricsArgs,
}

/// An opinionated [IndexerCluster] that spins up an [Indexer] implementation using Postgres as its
/// store, along with a [MetricsService] and a tracing subscriber (outputting to stderr) to provide
/// observability. It is a useful starting point for an indexer binary.
pub struct IndexerCluster {
    indexer: Indexer<Db>,
    metrics: MetricsService,

    /// Cancelling this token signals cancellation to both the indexer and metrics service.
    cancel: CancellationToken,
}

/// Builder for creating an IndexerCluster with a fluent API
#[derive(Default)]
pub struct IndexerClusterBuilder {
    database_url: Option<Url>,
    db_args: DbArgs,
    args: Args,
    ingestion_config: IngestionConfig,
    migrations: Option<&'static EmbeddedMigrations>,
    metric_label: Option<String>,
}

impl IndexerClusterBuilder {
    /// Create a new builder instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the PostgreSQL database connection URL (required).
    ///
    /// This should be a valid PostgreSQL connection urls, e.g.:
    /// - `postgres://user:password@host:5432/mydb`
    pub fn with_database_url(mut self, url: Url) -> Self {
        self.database_url = Some(url);
        self
    }

    /// Configure database connection parameters such as pool size, connection timeout, etc.
    ///
    /// Defaults to [`DbArgs::default()`] if not specified, which provides reasonable defaults
    /// for most use cases.
    pub fn with_db_args(mut self, args: DbArgs) -> Self {
        self.db_args = args;
        self
    }

    /// Set the main indexer cluster's configuration arguments (required).
    ///
    /// This bundles all configuration needed for the indexer:
    /// - `IndexerArgs`: Controls what to index (checkpoint range, which pipelines to run, watermark behavior)
    /// - `ClientArgs`: Specifies where to fetch checkpoint data from (remote store, local path, or RPC)
    /// - `MetricsArgs`: Configures how to expose Prometheus metrics (address to serve on)
    ///
    /// This overwrites any previously set args.
    pub fn with_args(mut self, args: Args) -> Self {
        self.args = args;
        self
    }

    /// Set indexer arguments (what to index and in what time range).
    /// This overwrites any previously set indexer args.
    pub fn with_indexer_args(mut self, args: IndexerArgs) -> Self {
        self.args.indexer_args = args;
        self
    }

    /// Set client arguments (where to get checkpoint data from).
    /// This overwrites any previously set client args.
    pub fn with_client_args(mut self, args: ClientArgs) -> Self {
        self.args.client_args = Some(args);
        self
    }

    /// Set metrics arguments (how to expose metrics).
    /// This overwrites any previously set metrics args.
    pub fn with_metrics_args(mut self, args: MetricsArgs) -> Self {
        self.args.metrics_args = args;
        self
    }

    /// Set the ingestion configuration, which controls how the ingestion service is
    /// set-up (its concurrency, polling, intervals, etc).
    pub fn with_ingestion_config(mut self, config: IngestionConfig) -> Self {
        self.ingestion_config = config;
        self
    }

    /// Set database migrations to run.
    ///
    /// See the [Diesel migration guide](https://diesel.rs/guides/migration_guide.html) for more information.
    pub fn with_migrations(mut self, migrations: &'static EmbeddedMigrations) -> Self {
        self.migrations = Some(migrations);
        self
    }

    /// Add a custom label to all metrics reported by this indexer instance.
    pub fn with_metric_label(mut self, label: impl Into<String>) -> Self {
        self.metric_label = Some(label.into());
        self
    }

    /// Build the IndexerCluster instance.
    ///
    /// Returns an error if:
    /// - Required fields are missing
    /// - Database connection cannot be established
    /// - Metrics registry creation fails
    pub async fn build(self) -> Result<IndexerCluster> {
        let database_url = self.database_url.context("database_url is required")?;

        tracing_subscriber::fmt::init();

        let cancel = CancellationToken::new();

        let registry = Registry::new_custom(self.metric_label, None)
            .context("Failed to create Prometheus registry.")?;

        let metrics = MetricsService::new(self.args.metrics_args, registry, cancel.child_token());
        let client_args = self.args.client_args.context("client_args is required")?;

        let indexer = Indexer::new_from_pg(
            database_url,
            self.db_args,
            self.args.indexer_args,
            client_args,
            self.ingestion_config,
            self.migrations,
            metrics.registry(),
            cancel.child_token(),
        )
        .await?;

        Ok(IndexerCluster {
            indexer,
            metrics,
            cancel,
        })
    }
}

impl IndexerCluster {
    /// Create a new builder for constructing an IndexerCluster.
    pub fn builder() -> IndexerClusterBuilder {
        IndexerClusterBuilder::new()
    }

    /// Access to the indexer's metrics. This can be cloned before a call to [Self::run], to retain
    /// shared access to the underlying metrics.
    pub fn metrics(&self) -> &Arc<IndexerMetrics> {
        self.indexer.metrics()
    }

    /// This token controls stopping the indexer and metrics service. Clone it before calling
    /// [Self::run] to retain the ability to stop the service after it has started.
    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Starts the indexer and metrics service, returning a handle to `await` the service's exit.
    /// The service will exit when the indexer has finished processing all the checkpoints it was
    /// configured to process, or when it receives an interrupt signal.
    pub async fn run(self) -> Result<JoinHandle<()>> {
        let h_ctrl_c = tokio::spawn({
            let cancel = self.cancel.clone();
            async move {
                tokio::select! {
                    _ = cancel.cancelled() => {}
                    _ = signal::ctrl_c() => {
                        info!("Received Ctrl-C, shutting down...");
                        cancel.cancel();
                    }
                }
            }
        });

        let h_metrics = self.metrics.run().await?;
        let h_indexer = self.indexer.run().await?;

        Ok(tokio::spawn(async move {
            let _ = h_indexer.await;
            self.cancel.cancel();
            let _ = h_metrics.await;
            let _ = h_ctrl_c.await;
        }))
    }
}

impl Deref for IndexerCluster {
    type Target = Indexer<Db>;

    fn deref(&self) -> &Self::Target {
        &self.indexer
    }
}

impl DerefMut for IndexerCluster {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.indexer
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use diesel::{Insertable, QueryDsl, Queryable};
    use diesel_async::RunQueryDsl;
    use sui_synthetic_ingestion::synthetic_ingestion;
    use tempfile::tempdir;

    use crate::ingestion::ClientArgs;
    use crate::pipeline::concurrent::{self, ConcurrentConfig};
    use crate::pipeline::Processor;
    use crate::postgres::{
        temp::{get_available_port, TempDb},
        Connection, Db, DbArgs,
    };
    use crate::types::full_checkpoint_content::CheckpointData;
    use crate::FieldCount;

    use super::*;

    diesel::table! {
        /// Table for storing transaction counts per checkpoint.
        tx_counts (cp_sequence_number) {
            cp_sequence_number -> BigInt,
            count -> BigInt,
        }
    }

    #[derive(Insertable, Queryable, FieldCount)]
    #[diesel(table_name = tx_counts)]
    struct StoredTxCount {
        cp_sequence_number: i64,
        count: i64,
    }

    /// Test concurrent pipeline for populating [tx_counts].
    struct TxCounts;

    impl Processor for TxCounts {
        const NAME: &'static str = "tx_counts";
        type Value = StoredTxCount;

        fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![StoredTxCount {
                cp_sequence_number: checkpoint.checkpoint_summary.sequence_number as i64,
                count: checkpoint.transactions.len() as i64,
            }])
        }
    }

    #[async_trait::async_trait]
    impl concurrent::Handler for TxCounts {
        type Store = Db;

        async fn commit<'a>(
            values: &[Self::Value],
            conn: &mut Connection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(diesel::insert_into(tx_counts::table)
                .values(values)
                .on_conflict_do_nothing()
                .execute(conn)
                .await?)
        }
    }

    #[tokio::test]
    async fn test_indexer_cluster() {
        let db = TempDb::new().expect("Failed to create temporary database");
        let url = db.database().url();

        // Generate test transactions to ingest.
        let checkpoint_dir = tempdir().unwrap();
        synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
            ingestion_dir: checkpoint_dir.path().to_owned(),
            starting_checkpoint: 0,
            num_checkpoints: 10,
            checkpoint_size: 2,
        })
        .await;

        let reader = Db::for_read(url.clone(), DbArgs::default()).await.unwrap();
        let writer = Db::for_write(url.clone(), DbArgs::default()).await.unwrap();

        {
            // Create the table we are going to write to. We have to do this manually, because this
            // table is not handled by migrations.
            let mut conn = writer.connect().await.unwrap();
            diesel::sql_query(
                r#"
                CREATE TABLE tx_counts (
                    cp_sequence_number  BIGINT PRIMARY KEY,
                    count               BIGINT NOT NULL
                )
                "#,
            )
            .execute(&mut conn)
            .await
            .unwrap();
        }

        let metrics_address =
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), get_available_port());

        let args = Args {
            client_args: Some(ClientArgs {
                local_ingestion_path: Some(checkpoint_dir.path().to_owned()),
                remote_store_url: None,
                rpc_api_url: None,
                rpc_username: None,
                rpc_password: None,
            }),
            indexer_args: IndexerArgs {
                first_checkpoint: Some(0),
                last_checkpoint: Some(9),
                ..Default::default()
            },
            metrics_args: MetricsArgs { metrics_address },
        };

        let mut indexer = IndexerCluster::builder()
            .with_database_url(url.clone())
            .with_args(args)
            .build()
            .await
            .unwrap();

        indexer
            .concurrent_pipeline(TxCounts, ConcurrentConfig::default())
            .await
            .unwrap();

        let metrics = indexer.metrics().clone();

        // Run the indexer until it signals completion. We have configured it to stop after
        // ingesting 10 checkpoints, so it should shut itself down.
        indexer.run().await.unwrap().await.unwrap();

        // Check that the results were all written out.
        {
            let mut conn = reader.connect().await.unwrap();
            let counts: Vec<StoredTxCount> = tx_counts::table
                .order_by(tx_counts::cp_sequence_number)
                .load(&mut conn)
                .await
                .unwrap();

            assert_eq!(counts.len(), 10);
            for (i, count) in counts.iter().enumerate() {
                assert_eq!(count.cp_sequence_number, i as i64);
                assert_eq!(count.count, 2);
            }
        }

        // Check that metrics were updated.
        assert_eq!(metrics.total_ingested_checkpoints.get(), 10);
        assert_eq!(metrics.total_ingested_transactions.get(), 20);
        assert_eq!(metrics.latest_ingested_checkpoint.get(), 9);

        macro_rules! assert_pipeline_metric {
            ($name:ident, $value:expr) => {
                assert_eq!(
                    metrics
                        .$name
                        .get_metric_with_label_values(&["tx_counts"])
                        .unwrap()
                        .get(),
                    $value
                );
            };
        }

        assert_pipeline_metric!(total_handler_checkpoints_received, 10);
        assert_pipeline_metric!(total_handler_checkpoints_processed, 10);
        assert_pipeline_metric!(total_handler_rows_created, 10);
        assert_pipeline_metric!(latest_processed_checkpoint, 9);
        assert_pipeline_metric!(total_collector_checkpoints_received, 10);
        assert_pipeline_metric!(total_collector_rows_received, 10);
        assert_pipeline_metric!(latest_collected_checkpoint, 9);

        // The watermark checkpoint is inclusive, but the transaction is exclusive
        assert_pipeline_metric!(watermark_checkpoint, 9);
        assert_pipeline_metric!(watermark_checkpoint_in_db, 9);
        assert_pipeline_metric!(watermark_transaction, 20);
        assert_pipeline_metric!(watermark_transaction_in_db, 20);
    }

    #[test]
    fn test_individual_methods_override_bundled_args() {
        let builder = IndexerClusterBuilder::new()
            .with_args(Args {
                indexer_args: IndexerArgs {
                    first_checkpoint: Some(100),
                    ..Default::default()
                },
                client_args: Some(ClientArgs {
                    local_ingestion_path: Some("/bundled".into()),
                    ..Default::default()
                }),
                metrics_args: MetricsArgs {
                    metrics_address: "127.0.0.1:8080".parse().unwrap(),
                },
            })
            .with_indexer_args(IndexerArgs {
                first_checkpoint: Some(200),
                ..Default::default()
            })
            .with_client_args(ClientArgs {
                local_ingestion_path: Some("/individual".into()),
                ..Default::default()
            })
            .with_metrics_args(MetricsArgs {
                metrics_address: "127.0.0.1:9090".parse().unwrap(),
            });

        assert_eq!(builder.args.indexer_args.first_checkpoint, Some(200));
        assert_eq!(
            builder
                .args
                .client_args
                .unwrap()
                .local_ingestion_path
                .unwrap()
                .to_string_lossy(),
            "/individual"
        );
        assert_eq!(
            builder.args.metrics_args.metrics_address.to_string(),
            "127.0.0.1:9090"
        );
    }

    #[test]
    fn test_bundled_args_override_individual_methods() {
        let builder = IndexerClusterBuilder::new()
            .with_indexer_args(IndexerArgs {
                first_checkpoint: Some(200),
                ..Default::default()
            })
            .with_client_args(ClientArgs {
                local_ingestion_path: Some("/individual".into()),
                ..Default::default()
            })
            .with_metrics_args(MetricsArgs {
                metrics_address: "127.0.0.1:9090".parse().unwrap(),
            })
            .with_args(Args {
                indexer_args: IndexerArgs {
                    first_checkpoint: Some(100),
                    ..Default::default()
                },
                client_args: Some(ClientArgs {
                    local_ingestion_path: Some("/bundled".into()),
                    ..Default::default()
                }),
                metrics_args: MetricsArgs {
                    metrics_address: "127.0.0.1:8080".parse().unwrap(),
                },
            });

        assert_eq!(builder.args.indexer_args.first_checkpoint, Some(100));
        assert_eq!(
            builder
                .args
                .client_args
                .unwrap()
                .local_ingestion_path
                .unwrap()
                .to_string_lossy(),
            "/bundled"
        );
        assert_eq!(
            builder.args.metrics_args.metrics_address.to_string(),
            "127.0.0.1:8080"
        );
    }
}
