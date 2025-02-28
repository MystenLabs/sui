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
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{
    db::DbArgs,
    ingestion::{ClientArgs, IngestionConfig},
    Indexer, IndexerArgs, IndexerMetrics, Result,
};

/// Bundle of arguments for setting up an indexer cluster (an Indexer and its associated Metrics
/// service). This struct is offered as a convenience for the common case of parsing command-line
/// arguments for a binary running a standalone indexer and its metrics service.
#[derive(clap::Parser, Debug)]
pub struct Args {
    /// What to index and in what time range.
    #[clap(flatten)]
    indexer_args: IndexerArgs,

    /// Where to get checkpoint data from.
    #[clap(flatten)]
    client_args: ClientArgs,

    /// How to expose metrics.
    #[clap(flatten)]
    metrics_args: MetricsArgs,
}

/// An [IndexerCluster] combines an [Indexer] with a [MetricsService] and a tracing subscriber
/// (outputting to stderr) to provide observability. It is a useful starting point for an indexer
/// binary.
pub struct IndexerCluster {
    indexer: Indexer,
    metrics: MetricsService,

    /// Cancelling this token signals cancellation to both the indexer and metrics service.
    cancel: CancellationToken,
}

impl IndexerCluster {
    /// Create a new cluster with most of the configuration set to its default value. Use
    /// [Self::new_with_configs] to construct a cluster with full customization.
    pub async fn new(
        database_url: Url,
        args: Args,
        migrations: Option<&'static EmbeddedMigrations>,
    ) -> Result<Self> {
        Self::new_with_configs(
            database_url,
            DbArgs::default(),
            args,
            IngestionConfig::default(),
            migrations,
            None,
        )
        .await
    }

    /// Create a new cluster.
    ///
    /// - `database_url` and `db_args` configure its database connection.
    /// - `args` configures where checkpoints are come from, what is indexed and metrics.
    /// - `ingestion_config` controls how the ingestion service is set-up (its concurrency, polling
    ///    intervals, etc).
    /// - `migrations` are any database migrations the indexer needs to run before starting to
    ///   ensure the database schema is ready for the data that is about to be committed.
    /// - `metric_label` is an optional custom label to add to metrics reported by this service.
    pub async fn new_with_configs(
        database_url: Url,
        db_args: DbArgs,
        args: Args,
        ingestion_config: IngestionConfig,
        migrations: Option<&'static EmbeddedMigrations>,
        metric_label: Option<String>,
    ) -> Result<Self> {
        tracing_subscriber::fmt::init();

        let cancel = CancellationToken::new();

        let registry = Registry::new_custom(metric_label, None)
            .context("Failed to create Prometheus registry.")?;

        let metrics = MetricsService::new(args.metrics_args, registry, cancel.child_token());

        let indexer = Indexer::new(
            database_url,
            db_args,
            args.indexer_args,
            args.client_args,
            ingestion_config,
            migrations,
            metrics.registry(),
            cancel.child_token(),
        )
        .await?;

        Ok(Self {
            indexer,
            metrics,
            cancel,
        })
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
        let h_metrics = self.metrics.run().await?;
        let h_indexer = self.indexer.run().await?;

        Ok(tokio::spawn(async move {
            let _ = h_indexer.await;
            self.cancel.cancel();
            let _ = h_metrics.await;
        }))
    }
}

impl Deref for IndexerCluster {
    type Target = Indexer;

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

    use crate::db::temp::{get_available_port, TempDb};
    use crate::db::{self, Db};
    use crate::pipeline::concurrent::{self, ConcurrentConfig};
    use crate::pipeline::Processor;
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
        async fn commit(
            values: &[Self::Value],
            conn: &mut db::Connection<'_>,
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
            client_args: ClientArgs {
                local_ingestion_path: Some(checkpoint_dir.path().to_owned()),
                remote_store_url: None,
            },
            indexer_args: IndexerArgs {
                first_checkpoint: Some(0),
                last_checkpoint: Some(9),
                ..Default::default()
            },
            metrics_args: MetricsArgs { metrics_address },
        };

        let mut indexer = IndexerCluster::new(url.clone(), args, None).await.unwrap();

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
}
