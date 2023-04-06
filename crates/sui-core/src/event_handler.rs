// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio_stream::Stream;
use tracing::{error, instrument, trace};

use sui_json_rpc_types::{EventFilter, SuiTransactionBlockEffects, SuiTransactionBlockEvents};
use sui_json_rpc_types::{SuiEvent, SuiTransactionBlockEffectsAPI};
use sui_types::error::SuiResult;

use crate::streamer::Streamer;

#[cfg(test)]
#[path = "unit_tests/event_handler_tests.rs"]
mod event_handler_tests;

pub const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub struct EventHandler {
    event_streamer: Streamer<SuiEvent, EventFilter>,
}

impl Default for EventHandler {
    fn default() -> Self {
        let streamer = Streamer::spawn(EVENT_DISPATCH_BUFFER_SIZE);
        Self {
            event_streamer: streamer,
        }
    }
}

impl EventHandler {
    #[instrument(level = "debug", skip_all, fields(tx_digest=?effects.transaction_digest()), err)]
    pub async fn process_events(
        &self,
        effects: &SuiTransactionBlockEffects,
        events: &SuiTransactionBlockEvents,
    ) -> SuiResult {
        trace!(
            num_events = events.data.len(),
            tx_digest =? effects.transaction_digest(),
            "Finished writing events to event store"
        );

        // serially dispatch event processing to honor events' orders.
        for event in events.data.clone() {
            if let Err(e) = self.event_streamer.send(event).await {
                error!(error =? e, "Failed to send event to dispatch");
            }
        }
        Ok(())
    }

    pub fn subscribe(&self, filter: EventFilter) -> impl Stream<Item = SuiEvent> {
        self.event_streamer.subscribe(filter)
    }
}
