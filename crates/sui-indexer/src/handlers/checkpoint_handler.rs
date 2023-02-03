// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use sui_sdk::SuiClient;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerCheckpointHandlerMetrics;
use sui_indexer::models::checkpoint_logs::{commit_checkpoint_log, read_checkpoint_log};
use sui_indexer::models::checkpoints::{
    commit_checkpoint, create_checkpoint, read_previous_checkpoint, Checkpoint,
};
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};
pub struct CheckpointHandler {
    rpc_client: SuiClient,
    pg_connection_pool: Arc<PgConnectionPool>,
    pub checkpoint_handler_metrics: IndexerCheckpointHandlerMetrics,
}

impl CheckpointHandler {
    pub fn new(
        rpc_client: SuiClient,
        pg_connection_pool: Arc<PgConnectionPool>,
        prometheus_registry: &Registry,
    ) -> Self {
        Self {
            rpc_client,
            pg_connection_pool,
            checkpoint_handler_metrics: IndexerCheckpointHandlerMetrics::new(prometheus_registry),
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer checkpoint handler started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;

        let checkpoint_log = read_checkpoint_log(&mut pg_pool_conn)?;
        let mut next_cursor_sequence_number = checkpoint_log.next_cursor_sequence_number;
        let mut previous_checkpoint_commit = if next_cursor_sequence_number != 0 {
            read_previous_checkpoint(&mut pg_pool_conn, next_cursor_sequence_number - 1).unwrap()
        } else {
            Checkpoint::default()
        };

        loop {
            self.checkpoint_handler_metrics
                .total_checkpoint_requested
                .inc();
            let mut checkpoint = self
                .rpc_client
                .read_api()
                .get_checkpoint_summary(next_cursor_sequence_number as u64)
                .await;
            // this happens very often b/c checkpoint indexing is faster than checkpoint
            // generation. Ideally we will want to differentiate between a real error and
            // a checkpoint not generated yet.
            while checkpoint.is_err() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                checkpoint = self
                    .rpc_client
                    .read_api()
                    .get_checkpoint_summary(next_cursor_sequence_number as u64)
                    .await;
            }
            self.checkpoint_handler_metrics
                .total_checkpoint_received
                .inc();
            // unwrap here is safe because we checked for error above
            let new_checkpoint = create_checkpoint(checkpoint.unwrap(), previous_checkpoint_commit);
            commit_checkpoint(&mut pg_pool_conn, new_checkpoint.clone())?;
            info!("Checkpoint {} committed", next_cursor_sequence_number);
            self.checkpoint_handler_metrics
                .total_checkpoint_processed
                .inc();

            previous_checkpoint_commit = Checkpoint::from(new_checkpoint.clone());
            next_cursor_sequence_number += 1;
            commit_checkpoint_log(&mut pg_pool_conn, next_cursor_sequence_number)?;
        }
    }
}
