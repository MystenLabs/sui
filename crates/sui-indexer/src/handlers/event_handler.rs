// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::{Base58, Encoding};
use prometheus::Registry;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::EventPage;
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::event::EventID;
use sui_types::query::EventQuery;
use tokio::time::sleep;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::IndexerEventHandlerMetrics;
use sui_indexer::models::event_logs::{commit_event_log, read_event_log};
use sui_indexer::models::events::commit_events;
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
        let event_log = read_event_log(&mut pg_pool_conn)?;
        let (tx_dig_opt, event_seq_opt) = (
            event_log.next_cursor_tx_dig,
            event_log.next_cursor_event_seq,
        );
        if let (Some(tx_dig), Some(_event_seq)) = (tx_dig_opt, event_seq_opt) {
            let tx_digest = TransactionDigest::from_str(&tx_dig).unwrap();
            // TODO: hack event_seq as 0 b/c of an event pagination bug, will change it
            // when the pagination bug is fixed.
            next_cursor = Some(EventID {
                tx_digest,
                event_seq: 0,
            });
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
            commit_events(&mut pg_pool_conn, event_page.clone())?;
            // Event page's next cursor can be None when latest event page is reached,
            // if we use the None cursor to read events, it will start from genesis,
            // thus here we do not commit / use the None cursor.
            // This will cause duplicate run of the current batch, but will not cause
            // duplicate rows b/c of the uniqueness restriction of the table.
            if let Some(next_cursor_val) = event_page.next_cursor.clone() {
                commit_event_log(
                    &mut pg_pool_conn,
                    Some(Base58::encode(next_cursor_val.tx_digest)),
                    Some(next_cursor_val.event_seq),
                )?;
                self.event_handler_metrics
                    .total_events_processed
                    .inc_by(event_count as u64);
                // TODO: hack event_seq as 0 b/c of an event pagination bug, change it
                // when the pagination bug is fixed.
                next_cursor = Some(EventID {
                    tx_digest: next_cursor_val.tx_digest,
                    event_seq: 0,
                });
            }
            self.event_handler_metrics.total_event_page_committed.inc();
            // sleep when the event page has been the latest page
            if event_count < EVENT_PAGE_SIZE || event_page.next_cursor.is_none() {
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
