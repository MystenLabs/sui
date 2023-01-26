// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::sync::Arc;
use sui_json_rpc_types::EventPage;
use sui_sdk::SuiClient;
use sui_types::event::{EventID, EventType};
use sui_types::query::EventQuery;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerPublishEventHandlerMetrics;
use sui_indexer::models::publish_event_logs::{commit_event_log, read_event_log};
use sui_indexer::models::publish_events::commit_events;
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

const EVENT_PAGE_SIZE: usize = 10;

pub struct PublishEventHandler {
    rpc_client: SuiClient,
    pg_connection_pool: Arc<PgConnectionPool>,
    pub event_handler_metrics: IndexerPublishEventHandlerMetrics,
}

impl PublishEventHandler {
    pub fn new(
        rpc_client: SuiClient,
        pg_connection_pool: Arc<PgConnectionPool>,
        prometheus_registry: &Registry,
    ) -> Self {
        let event_handler_metrics = IndexerPublishEventHandlerMetrics::new(prometheus_registry);
        Self {
            rpc_client,
            pg_connection_pool,
            event_handler_metrics,
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer publish event handler started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;

        let mut next_cursor = None;
        let event_log = read_event_log(&mut pg_pool_conn)?;
        let (tx_dig_opt, event_seq_opt) = (
            event_log.next_cursor_tx_dig,
            event_log.next_cursor_event_seq,
        );
        if let (Some(tx_dig), Some(event_seq)) = (tx_dig_opt, event_seq_opt) {
            let tx_digest = tx_dig.parse().map_err(|e| {
                IndexerError::TransactionDigestParsingError(format!(
                    "Failed parsing transaction digest {:?} with error: {:?}",
                    tx_dig, e
                ))
            })?;
            next_cursor = Some(EventID {
                tx_digest,
                event_seq,
            });
        }

        loop {
            let event_page =
                fetch_publish_event_page(self.rpc_client.clone(), next_cursor.clone()).await?;
            let event_count = event_page.data.len();
            self.event_handler_metrics
                .total_publish_events_received
                .inc_by(event_count as u64);
            info!(
                "Received publish event page with {} events, next cursor: {:?}",
                event_count, event_page.next_cursor
            );

            commit_events(&mut pg_pool_conn, event_page.clone())?;
            if let Some(next_cursor_val) = event_page.next_cursor.clone() {
                commit_event_log(
                    &mut pg_pool_conn,
                    Some(next_cursor_val.tx_digest.base58_encode()),
                    Some(next_cursor_val.event_seq),
                )?;
                self.event_handler_metrics
                    .total_publish_events_processed
                    .inc_by(event_count as u64);
                next_cursor = Some(next_cursor_val);
            }
        }
    }
}

async fn fetch_publish_event_page(
    rpc_client: SuiClient,
    next_cursor: Option<EventID>,
) -> Result<EventPage, IndexerError> {
    rpc_client
        .event_api()
        .get_events(
            EventQuery::EventType(EventType::Publish),
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
