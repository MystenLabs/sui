// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use sui_sdk::SuiClient;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerCheckpointHandlerMetrics;
use sui_indexer::models::checkpoint_logs::{commit_checkpoint_log, read_checkpoint_log};
use sui_indexer::models::checkpoints::commit_checkpoint;
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

        loop {
            self.checkpoint_handler_metrics
                .total_checkpoint_requested
                .inc();
            let checkpoint = self
                .rpc_client
                .read_api()
                .get_checkpoint_summary(next_cursor_sequence_number as u64)
                .await
                .map_err(|e| {
                    IndexerError::FullNodeReadingError(format!(
                        "Failed to get checkpoint with sequence {} error: {:?}",
                        next_cursor_sequence_number, e
                    ))
                })?;
            self.checkpoint_handler_metrics
                .total_checkpoint_received
                .inc();
            commit_checkpoint(&mut pg_pool_conn, checkpoint)?;
            info!("Checkpoint {} committed", next_cursor_sequence_number);
            self.checkpoint_handler_metrics
                .total_checkpoint_processed
                .inc();

            next_cursor_sequence_number += 1;
            commit_checkpoint_log(&mut pg_pool_conn, next_cursor_sequence_number)?;
        }
    }
}
