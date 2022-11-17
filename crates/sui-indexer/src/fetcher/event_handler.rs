// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::EventPage;
use sui_sdk::SuiClient;
use sui_types::event::EventID;
use sui_types::query::EventQuery;
use tracing::{info, warn};

const EVENT_PAGE_SIZE: usize = 100;

pub struct EventHandler {
    rpc_client: SuiClient,
}

impl EventHandler {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self { rpc_client }
    }

    pub async fn run_forever(&self) {
        // TODO: read next cursor from DB if available
        let mut next_cursor = None;
        loop {
            let event_page = self.fetch_event_page(next_cursor).await;
            if let Ok(event_page) = event_page {
                info!("Current event page: {:?}", event_page.data);
                // TODO: write this page of events to DB
                next_cursor = event_page.next_cursor;
            } else {
                warn!("Get event page failed: {:?}", event_page);
                break;
            }
        }
    }

    async fn fetch_event_page(&self, next_cursor: Option<EventID>) -> anyhow::Result<EventPage> {
        self.rpc_client
            .event_api()
            .get_events(EventQuery::All, next_cursor, Some(EVENT_PAGE_SIZE), None)
            .await
    }
}
