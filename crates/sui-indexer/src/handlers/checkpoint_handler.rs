// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use sui_sdk::SuiClient;
use tracing::{error, info};

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerCheckpointHandlerMetrics;
use sui_indexer::models::checkpoints::{
    commit_checkpoint, create_checkpoint, get_checkpoint, get_latest_checkpoint_sequence_number,
    Checkpoint,
};
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};
use sui_json_rpc_types::CheckpointId;

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
        let mut next_cursor_sequence_number =
            get_latest_checkpoint_sequence_number(&mut pg_pool_conn)? + 1;
        let mut previous_checkpoint = Checkpoint::default();
        if next_cursor_sequence_number > 0 {
            let temp_checkpoint =
                get_checkpoint(&mut pg_pool_conn, next_cursor_sequence_number - 1);
            match temp_checkpoint {
                Ok(checkpoint) => previous_checkpoint = checkpoint,
                Err(err) => {
                    error!("{}", err)
                }
            }
        }

        loop {
            self.checkpoint_handler_metrics
                .total_checkpoint_requested
                .inc();

            let request_guard = self
                .checkpoint_handler_metrics
                .full_node_read_request_latency
                .start_timer();

            let next_cursor_checkpoint_id =
                CheckpointId::SequenceNumber(next_cursor_sequence_number as u64);
            let mut checkpoint = self
                .rpc_client
                .read_api()
                .get_checkpoint(next_cursor_checkpoint_id.clone())
                .await;
            // this happens very often b/c checkpoint indexing is faster than checkpoint
            // generation. Ideally we will want to differentiate between a real error and
            // a checkpoint not generated yet.
            while checkpoint.is_err() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                checkpoint = self
                    .rpc_client
                    .read_api()
                    .get_checkpoint(next_cursor_checkpoint_id.clone())
                    .await;
            }
            request_guard.stop_and_record();

            self.checkpoint_handler_metrics
                .total_checkpoint_received
                .inc();

            let db_guard = self
                .checkpoint_handler_metrics
                .db_write_request_latency
                .start_timer();
            // unwrap here is safe because we checked for error above
            let new_checkpoint = create_checkpoint(checkpoint.unwrap(), previous_checkpoint)?;
            commit_checkpoint(&mut pg_pool_conn, new_checkpoint.clone())?;
            info!("Checkpoint {} committed", next_cursor_sequence_number);
            self.checkpoint_handler_metrics
                .total_checkpoint_processed
                .inc();
            db_guard.stop_and_record();
            previous_checkpoint = Checkpoint::from(new_checkpoint.clone());
            next_cursor_sequence_number += 1;
        }
    }
}
