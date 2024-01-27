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
    indexer::{format_update_field_query, SuinsIndexer},
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
    /// Commits the list of VerifiedDomains into the db.
    fn commit_updates_to_db(
        &self,
        updates: &Vec<VerifiedDomain>,
        deletions: &Vec<String>,
    ) -> Result<()> {
        if updates.is_empty() && deletions.is_empty() {
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
                        domains::subdomain_wrapper_id
                            .eq(sql(&format_update_field_query("subdomain_wrapper_id"))),
                    ))
                    .execute(tx)
                    .unwrap_or_else(|_| panic!("Failed to process updates: {:?}", updates));
            }

            if !deletions.is_empty() {
                diesel::delete(domains::table)
                    .filter(domains::field_id.eq_any(deletions))
                    .execute(tx)
                    .unwrap_or_else(|_| panic!("Failed to process deletions: {:?}", deletions));
            }

            Ok(())
        })
    }
}

#[async_trait]
impl Worker for SuinsIndexerWorker {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()> {
        let (db_updates, removals) = self.indexer.process_checkpoint(checkpoint);

        self.commit_updates_to_db(&db_updates, &removals)?;
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
            },
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
            ],
            exit_receiver,
        )
        .await?;
    Ok(())
}
