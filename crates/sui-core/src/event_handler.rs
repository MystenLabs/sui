// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_bytecode_utils::module_cache::SyncModuleCache;
use tokio_stream::Stream;
use tracing::{debug, error, instrument, trace};

use sui_json_rpc_types::SuiMoveStruct;
use sui_types::base_types::TransactionDigest;
use sui_types::filter::EventFilter;
use sui_types::messages::TransactionEvents;
use sui_types::{
    error::{SuiError, SuiResult},
    event::{Event, EventEnvelope},
    messages::{TransactionEffects, TransactionEffectsAPI},
};

use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::streamer::Streamer;

#[cfg(test)]
#[path = "unit_tests/event_handler_tests.rs"]
mod event_handler_tests;

pub const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub struct EventHandler {
    event_streamer: Streamer<EventEnvelope, EventFilter>,
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
    #[instrument(level = "debug", skip_all, fields(seq=?seq_num, tx_digest=?effects.transaction_digest()), err)]
    pub async fn process_events(
        &self,
        effects: &TransactionEffects,
        events: &TransactionEvents,
        timestamp_ms: u64,
        seq_num: u64,
        module_cache: &SyncModuleCache<ResolverWrapper<AuthorityStore>>,
    ) -> SuiResult {
        let res: Result<Vec<_>, _> = events
            .data
            .iter()
            .enumerate()
            .map(|(event_num, e)| {
                self.create_envelope(
                    e,
                    *effects.transaction_digest(),
                    event_num.try_into().unwrap(),
                    seq_num,
                    timestamp_ms,
                    module_cache,
                )
            })
            .collect();
        let envelopes = res?;

        trace!(
            num_events = envelopes.len(),
            tx_digest =? effects.transaction_digest(),
            "Finished writing events to event store"
        );

        // serially dispatch event processing to honor events' orders.
        for envelope in envelopes {
            if let Err(e) = self.event_streamer.send(envelope).await {
                error!(error =? e, "Failed to send EventEnvelope to dispatch");
            }
        }

        Ok(())
    }

    fn create_envelope(
        &self,
        event: &Event,
        digest: TransactionDigest,
        event_num: u64,
        seq_num: u64,
        timestamp_ms: u64,
        module_cache: &SyncModuleCache<ResolverWrapper<AuthorityStore>>,
    ) -> Result<EventEnvelope, SuiError> {
        let json_value = match event {
            Event::MoveEvent {
                type_, contents, ..
            } => {
                debug!(event =? event, "Process MoveEvent.");
                let move_struct = Event::move_event_to_move_struct(type_, contents, module_cache)?;
                // Convert into `SuiMoveStruct` which is a mirror of MoveStruct but with additional type supports, (e.g. ascii::String).
                let sui_move_struct = SuiMoveStruct::from(move_struct);
                Some(sui_move_struct.to_json_value())
            }
            _ => None,
        };

        Ok(EventEnvelope::new(
            timestamp_ms,
            digest,
            seq_num,
            event_num,
            event.clone(),
            json_value,
        ))
    }

    pub fn subscribe(&self, filter: EventFilter) -> impl Stream<Item = EventEnvelope> {
        self.event_streamer.subscribe(filter)
    }
}
