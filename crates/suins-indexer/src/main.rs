// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use diesel::{dsl::sql, BoolExpressionMethods, ExpressionMethods};
use diesel_async::{scoped_futures::ScopedFutureExt, AsyncConnection, RunQueryDsl};
use dotenvy::dotenv;
use mysten_service::metrics::start_basic_prometheus_server;
use prometheus::Registry;
use std::env;
use std::path::PathBuf;
use sui_data_ingestion_core::{
    DataIngestionMetrics, FileProgressStore, IndexerExecutor, ReaderOptions, Worker, WorkerPool,
};
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::sync::oneshot;
use tracing::info;

use suins_indexer::{
    get_connection_pool,
    indexer::{format_update_field_query, format_update_subdomain_wrapper_query, SuinsIndexer},
    models::VerifiedDomain,
    schema::domains,
    PgConnectionPool,
};

struct SuinsIndexerWorker {
    pg_pool: PgConnectionPool,
    indexer: SuinsIndexer,
}

impl SuinsIndexerWorker {
    /// Creates a transcation that upserts the given name record updates,
    /// and deletes the given name record deletions.
    ///
    /// This is done using 1 or 2 queries, depending on whether there are any deletions/updates in the checkpoint.
    ///
    /// - The first query is a bulk insert of all updates, with an upsert on conflict.
    /// - The second query is a bulk delete of all deletions.
    ///
    /// You can safely call this with empty updates/deletions as it will return Ok.
    async fn commit_to_db(
        &self,
        updates: &[VerifiedDomain],
        removals: &[String],
        checkpoint_seq_num: u64,
    ) -> Result<()> {
        if updates.is_empty() && removals.is_empty() {
            return Ok(());
        }

        let mut connection = self.pg_pool.get().await.unwrap();

        connection
            .transaction::<_, anyhow::Error, _>(|conn| {
                async move {
                    if !updates.is_empty() {
                        // Bulk insert all updates and override with data.
                        diesel::insert_into(domains::table)
                            .values(updates)
                            .on_conflict(domains::name)
                            .do_update()
                            .set((
                                domains::expiration_timestamp_ms
                                    .eq(sql(&format_update_field_query("expiration_timestamp_ms"))),
                                domains::nft_id.eq(sql(&format_update_field_query("nft_id"))),
                                domains::target_address
                                    .eq(sql(&format_update_field_query("target_address"))),
                                domains::data.eq(sql(&format_update_field_query("data"))),
                                domains::last_checkpoint_updated
                                    .eq(sql(&format_update_field_query("last_checkpoint_updated"))),
                                domains::field_id.eq(sql(&format_update_field_query("field_id"))),
                                // We always want to respect the subdomain_wrapper re-assignment, even if the checkpoint is older.
                                // That prevents a scenario where we first process a later checkpoint that did an update to the name record (e..g change target address),
                                // without first executing the checkpoint that created the subdomain wrapper.
                                // Since wrapper re-assignment can only happen every 2 days, we can't write invalid data here.
                                domains::subdomain_wrapper_id
                                    .eq(sql(&format_update_subdomain_wrapper_query())),
                            ))
                            .execute(conn)
                            .await
                            .unwrap_or_else(|_| panic!("Failed to process updates: {:?}", updates));
                    }

                    if !removals.is_empty() {
                        // We want to remove from the database all name records that were removed in the checkpoint
                        // but only if the checkpoint is newer than the last time the name record was updated.
                        diesel::delete(domains::table)
                            .filter(domains::field_id.eq_any(removals).and(
                                domains::last_checkpoint_updated.le(checkpoint_seq_num as i64),
                            ))
                            .execute(conn)
                            .await
                            .unwrap_or_else(|_| {
                                panic!("Failed to process deletions: {:?}", removals)
                            });
                    }

                    Ok(())
                }
                .scope_boxed()
            })
            .await
    }
}

#[async_trait]
impl Worker for SuinsIndexerWorker {
    type Result = ();
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> Result<()> {
        let checkpoint_seq_number = checkpoint.checkpoint_summary.sequence_number;
        let (updates, removals) = self.indexer.process_checkpoint(checkpoint);

        // every 1000 checkpoints, we will print the checkpoint sequence number
        // to the console to keep track of progress
        if checkpoint_seq_number % 1000 == 0 {
            info!("Checkpoint sequence number: {}", checkpoint_seq_number);
        }
        self.commit_to_db(&updates, &removals, checkpoint_seq_number)
            .await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = mysten_service::logging::init();
    dotenv().ok();
    let (remote_storage, registry_id, subdomain_wrapper_type, name_record_type) = (
        env::var("REMOTE_STORAGE").ok(),
        env::var("REGISTRY_ID").ok(),
        env::var("SUBDOMAIN_WRAPPER_TYPE").ok(),
        env::var("NAME_RECORD_TYPE").ok(),
    );
    let backfill_progress_file_path =
        env::var("BACKFILL_PROGRESS_FILE_PATH").unwrap_or("/tmp/backfill_progress".to_string());
    let checkpoints_dir = env::var("CHECKPOINTS_DIR").unwrap_or("/tmp/checkpoints".to_string());

    println!("Starting indexer with checkpoints dir: {}", checkpoints_dir);

    let (_exit_sender, exit_receiver) = oneshot::channel();
    let progress_store = FileProgressStore::new(PathBuf::from(backfill_progress_file_path));

    let registry: Registry = start_basic_prometheus_server();
    mysten_metrics::init_metrics(&registry);
    let metrics = DataIngestionMetrics::new(&registry);
    let mut executor = IndexerExecutor::new(progress_store, 1, metrics);

    let indexer_setup =
        if let (Some(registry_id), Some(subdomain_wrapper_type), Some(name_record_type)) =
            (registry_id, subdomain_wrapper_type, name_record_type)
        {
            SuinsIndexer::new(registry_id, subdomain_wrapper_type, name_record_type)
        } else {
            SuinsIndexer::default()
        };

    let worker_pool = WorkerPool::new(
        SuinsIndexerWorker {
            pg_pool: get_connection_pool().await,
            indexer: indexer_setup,
        },
        "suins_indexing".to_string(), /* task name used as a key in the progress store */
        100,                          /* concurrency */
    );
    executor.register(worker_pool).await?;

    executor
        .run(
            PathBuf::from(checkpoints_dir), /* directory should exist but can be empty */
            remote_storage,                 /* remote_read_endpoint: If set */
            vec![],                         /* aws credentials */
            ReaderOptions::default(),       /* remote_read_batch_size */
            exit_receiver,
        )
        .await?;
    drop(_guard);
    Ok(())
}
