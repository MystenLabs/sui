// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::EventPage;
use sui_sdk::SuiClient;
use sui_types::event::EventID;
use sui_types::query::EventQuery;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::establish_connection;
use sui_indexer::models::event_logs::{commit_event_log, read_event_log};
use sui_indexer::models::events::commit_events;

const EVENT_PAGE_SIZE: usize = 100;

pub struct EventHandler {
    rpc_client: SuiClient,
}

impl EventHandler {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self { rpc_client }
    }

    pub async fn start(&self) -> Result<(), IndexerError> {
        info!("Indexer event handler started...");
        let mut pg_conn = establish_connection();
        let mut next_cursor = None;
        let event_log = read_event_log(&mut pg_conn)?;
        let (tx_seq_opt, event_seq_opt) = (
            event_log.next_cursor_tx_seq,
            event_log.next_cursor_event_seq,
        );
        if let (Some(tx_seq), Some(event_seq)) = (tx_seq_opt, event_seq_opt) {
            next_cursor = Some(EventID { tx_seq, event_seq });
        }

        loop {
            let event_page = self.fetch_event_page(next_cursor).await?;
            commit_events(&mut pg_conn, event_page.clone())?;
            commit_event_log(
                &mut pg_conn,
                event_page.next_cursor.clone().map(|c| c.tx_seq),
                event_page.next_cursor.clone().map(|c| c.event_seq),
            )?;
            next_cursor = event_page.next_cursor;
        }
    }

    async fn fetch_event_page(
        &self,
        next_cursor: Option<EventID>,
    ) -> Result<EventPage, IndexerError> {
        self.rpc_client
            .event_api()
            .get_events(
                EventQuery::All,
                next_cursor.clone(),
                Some(EVENT_PAGE_SIZE),
                None,
            )
            .await
            .map_err(|e| {
                IndexerError::FullNodeReadingError(format!(
                    "Failed reading event page with cursor {:?} and error: {:?}",
                    next_cursor, e
                ))
            })
    }
}
