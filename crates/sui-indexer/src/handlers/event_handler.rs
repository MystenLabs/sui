// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionResponse;
use sui_sdk::SuiClient;
use tokio::time::sleep;
use tracing::{error, info};

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerEventHandlerMetrics;
use sui_indexer::models::events::{commit_events, read_last_event, IndexerEventEnvelope};
use sui_indexer::utils::log_errors_to_pg;
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

use crate::handlers::transaction_handler::{get_transaction_page, get_transaction_response};

pub struct EventHandler {
    rpc_client: SuiClient,
    pg_connection_pool: Arc<PgConnectionPool>,
    pub event_handler_metrics: IndexerEventHandlerMetrics,
}

impl EventHandler {
    pub fn new(
        rpc_client: SuiClient,
        pg_connection_pool: Arc<PgConnectionPool>,
        prometheus_registry: &Registry,
    ) -> Self {
        let event_handler_metrics = IndexerEventHandlerMetrics::new(prometheus_registry);
        Self {
            rpc_client,
            pg_connection_pool,
            event_handler_metrics,
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer event handler started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;

        let mut next_cursor = None;
        let last_event_opt = read_last_event(&mut pg_pool_conn)?;
        if let Some(last_event) = last_event_opt {
            if let Some(next_cursor_tx_dig) = last_event.next_cursor_transaction_digest {
                let next_cursor_tx_digest = next_cursor_tx_dig.parse().map_err(|e| {
                    IndexerError::TransactionDigestParsingError(format!(
                        "Failed parsing transaction digest {:?} with error: {:?}",
                        next_cursor_tx_dig, e
                    ))
                })?;
                next_cursor = Some(next_cursor_tx_digest);
            } else {
                error!("Last event was found but it has no next cursor tx digest, this should never happen!");
            }
        }

        loop {
            self.event_handler_metrics
                .total_event_page_fetch_attempt
                .inc();

            let request_guard = self
                .event_handler_metrics
                .full_node_read_request_latency
                .start_timer();

            let txn_page = get_transaction_page(self.rpc_client.clone(), next_cursor).await?;
            let txn_digest_vec = txn_page.data;
            let txn_response_res_vec = join_all(
                txn_digest_vec
                    .into_iter()
                    .map(|tx_digest| get_transaction_response(self.rpc_client.clone(), tx_digest)),
            )
            .await;
            self.event_handler_metrics.total_event_page_received.inc();
            info!(
                "Received transaction page for events with {} txns, next cursor: {:?}",
                txn_response_res_vec.len(),
                txn_page.next_cursor.clone()
            );
            request_guard.stop_and_record();

            let db_guard = self
                .event_handler_metrics
                .db_write_request_latency
                .start_timer();
            let mut errors = vec![];
            let txn_resp_vec: Vec<SuiTransactionResponse> = txn_response_res_vec
                .into_iter()
                .filter_map(|f| f.map_err(|e| errors.push(e)).ok())
                .collect();
            let event_nested_vec: Vec<IndexerEventEnvelope> = txn_resp_vec
                .into_iter()
                .map(|txn_resp| IndexerEventEnvelope {
                    transaction_digest: txn_resp.effects.transaction_digest,
                    timestamp: txn_resp.timestamp_ms,
                    events: txn_resp.effects.events,
                    next_cursor: None,
                })
                .collect();
            log_errors_to_pg(&mut pg_pool_conn, errors);
            let commit_result =
                commit_events(&mut pg_pool_conn, event_nested_vec, txn_page.next_cursor)?;

            if let Some((commit_count, next_cursor_val)) = commit_result {
                next_cursor = Some(next_cursor_val);

                self.event_handler_metrics.total_event_page_committed.inc();
                self.event_handler_metrics
                    .total_events_processed
                    .inc_by(commit_count as u64);
                info!(
                    "Committed {} events, next cursor: {:?}",
                    commit_count, next_cursor
                );
            }
            db_guard.stop_and_record();

            if txn_page.next_cursor.is_none() {
                sleep(Duration::from_secs_f32(0.1)).await;
            }
        }
    }
}
