// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::EventPage;
use sui_sdk::SuiClient;
use sui_types::event::EventID;
use sui_types::query::EventQuery;
use tokio::time::sleep;
use tracing::{error, info};

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerEventHandlerMetrics;
use sui_indexer::models::events::{commit_events, read_last_event};
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

const EVENT_PAGE_SIZE: usize = 100;

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
            match (
                last_event.next_cursor_transaction_digest,
                last_event.next_cursor_event_sequence,
            ) {
                (Some(tx_digest_str), Some(event_seq)) => {
                    let tx_digest = tx_digest_str.parse().map_err(|e| {
                        IndexerError::TransactionDigestParsingError(format!(
                            "Failed parsing transaction digest {:?} with error: {:?}",
                            tx_digest_str, e
                        ))
                    })?;
                    next_cursor = Some(EventID {
                        tx_digest,
                        event_seq,
                    });
                }
                (Some(_), None) => {
                    error!("Last event was found but it has no next cursor event sequence, this should never happen!");
                }
                (None, Some(_)) => {
                    error!("Last event was found but it has no next cursor tx digest, this should never happen!");
                }
                (None, None) => {
                    error!("Last event was found but it has no next cursor tx digest and no next cursor event sequence, this should never happen!");
                }
            }
        }

        loop {
            self.event_handler_metrics
                .total_event_page_fetch_attempt
                .inc();
            let event_page = fetch_event_page(self.rpc_client.clone(), next_cursor.clone()).await?;
            self.event_handler_metrics.total_event_page_received.inc();
            let event_count = event_page.data.len();
            info!(
                "Received event page with {} events, next cursor: {:?}",
                event_count, event_page.next_cursor
            );
            self.event_handler_metrics
                .total_events_received
                .inc_by(event_count as u64);
            let commit_result = commit_events(&mut pg_pool_conn, event_page.clone())?;

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

            if event_page.next_cursor.is_none() {
                sleep(Duration::from_secs_f32(0.1)).await;
            }
        }
    }
}

async fn fetch_event_page(
    rpc_client: SuiClient,
    next_cursor: Option<EventID>,
) -> Result<EventPage, IndexerError> {
    rpc_client
        .event_api()
        .get_events(
            EventQuery::All,
            next_cursor.clone(),
            Some(EVENT_PAGE_SIZE),
            false,
        )
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed reading event page with cursor {:?} and error: {:?}",
                next_cursor, e
            ))
        })
}
