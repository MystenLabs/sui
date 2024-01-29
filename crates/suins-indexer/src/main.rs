// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use diesel::{dsl::sql, Connection, ExpressionMethods, RunQueryDsl};
use prometheus::Registry;
use std::path::PathBuf;
use sui_data_ingestion::{
    DataIngestionMetrics, FileProgressStore, IndexerExecutor, Worker, WorkerPool,
};
use sui_types::full_checkpoint_content::CheckpointData;
use suins_indexer::{
    get_connection_pool,
    indexer::{format_update_field_query, format_update_subdomain_wrapper_query, SuinsIndexer},
    models::VerifiedDomain,
    schema::domains,
    PgConnectionPool,
};

use dotenvy::dotenv;
use std::env;
use tokio::sync::oneshot;

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
    fn commit_to_db(&self, updates: &Vec<VerifiedDomain>, removals: &Vec<String>) -> Result<()> {
        if updates.is_empty() && removals.is_empty() {
            return Ok(());
        }

        let connection = &mut self.pg_pool.get().unwrap();

        connection.transaction(|tx| {
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
                        //
                        domains::subdomain_wrapper_id
                            .eq(sql(&format_update_subdomain_wrapper_query())),
                    ))
                    .execute(tx)
                    .unwrap_or_else(|_| panic!("Failed to process updates: {:?}", updates));
            }

            if !removals.is_empty() {
                diesel::delete(domains::table)
                    .filter(domains::field_id.eq_any(removals))
                    .execute(tx)
                    .unwrap_or_else(|_| panic!("Failed to process deletions: {:?}", removals));
            }

            Ok(())
        })
    }
}

#[async_trait]
impl Worker for SuinsIndexerWorker {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()> {
        let (updates, removals) = self.indexer.process_checkpoint(checkpoint);

        self.commit_to_db(&updates, &removals)?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let aws_key_id = env::var("AWS_ACCESS_KEY_ID").ok();
    let aws_secret_access_key = env::var("AWS_ACCESS_SECRET_KEY").ok();
    let aws_session_token = env::var("AWS_SESSION_TOKEN").ok();
    let backfill_progress_file_path =
        env::var("BACKFILL_PROGRESS_FILE_PATH").unwrap_or("/tmp/backfill_progress".to_string());
    let checkpoints_dir = env::var("CHECKPOINTS_DIR").unwrap_or("/tmp/checkpoints".to_string());

    println!("Starting indexer with checkpoints dir: {}", checkpoints_dir);

    let (_exit_sender, exit_receiver) = oneshot::channel();
    let progress_store = FileProgressStore::new(PathBuf::from(backfill_progress_file_path));
    let metrics = DataIngestionMetrics::new(&Registry::new());
    let mut executor = IndexerExecutor::new(progress_store, 1, metrics);

    let worker_pool = WorkerPool::new(
        SuinsIndexerWorker {
            pg_pool: get_connection_pool(),
            // TODO(manos): This should be configurable from env.
            indexer: SuinsIndexer::default(),
        },
        "suins_indexing".to_string(), /* task name used as a key in the progress store */
        100,                          /* concurrency */
    );
    executor.register(worker_pool).await?;

    executor
        .run(
            PathBuf::from(checkpoints_dir), /* directory should exist but can be empty */
            if aws_key_id.is_some() {
                Some("https://s3.us-west-2.amazonaws.com/mysten-mainnet-checkpoints".to_string())
            } else {
                None
            }, /* remote_read_endpoint: If set */
            vec![
                (
                    "aws_access_key_id".to_string(),
                    aws_key_id.unwrap_or("".to_string()),
                ),
                (
                    "aws_secret_access_key".to_string(),
                    aws_secret_access_key.unwrap_or("".to_string()),
                ),
                (
                    "aws_session_token".to_string(),
                    aws_session_token.unwrap_or("".to_string()),
                ),
            ], /* aws credentials */
            100,                            /* remote_read_batch_size */
            exit_receiver,
        )
        .await?;
    Ok(())
}
