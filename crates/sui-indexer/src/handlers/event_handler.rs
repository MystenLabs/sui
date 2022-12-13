// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::EventPage;
use sui_sdk::SuiClient;
use sui_types::event::EventID;
use sui_types::query::EventQuery;
use tokio::time::sleep;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::models::event_logs::{commit_event_log, read_event_log};
use sui_indexer::models::events::commit_events;
use sui_indexer::{get_pg_pool_connection, PgConnectionPool};

const EVENT_PAGE_SIZE: usize = 100;

pub struct EventHandler {
    rpc_client: SuiClient,
    pg_connection_pool: Arc<PgConnectionPool>,
}

impl EventHandler {
    pub fn new(rpc_client: SuiClient, pg_connection_pool: Arc<PgConnectionPool>) -> Self {
        Self {
            rpc_client,
            pg_connection_pool,
        }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer event handler started...");
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;

        let mut next_cursor = None;
        let event_log = read_event_log(&mut pg_pool_conn)?;
        let (tx_seq_opt, event_seq_opt) = (
            event_log.next_cursor_tx_seq,
            event_log.next_cursor_event_seq,
        );
        if let (Some(tx_seq), Some(event_seq)) = (tx_seq_opt, event_seq_opt) {
            next_cursor = Some(EventID { tx_seq, event_seq });
        }

        loop {
            let event_page = fetch_event_page(self.rpc_client.clone(), next_cursor).await?;
            let event_count = event_page.data.len();
            commit_events(&mut pg_pool_conn, event_page.clone())?;
            commit_event_log(
                &mut pg_pool_conn,
                event_page.next_cursor.clone().map(|c| c.tx_seq),
                event_page.next_cursor.clone().map(|c| c.event_seq),
            )?;
            next_cursor = event_page.next_cursor;
            if event_count < EVENT_PAGE_SIZE {
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
