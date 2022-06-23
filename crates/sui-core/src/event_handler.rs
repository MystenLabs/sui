// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_bytecode_utils::module_cache::SyncModuleCache;
use sui_types::base_types::TransactionDigest;
use tokio_stream::Stream;
use tracing::{debug, error};

use sui_storage::event_store::EventStore;
use sui_types::object::ObjectFormatOptions;
use sui_types::{
    error::{SuiError, SuiResult},
    event::{Event, EventEnvelope},
    messages::TransactionEffects,
};

use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::event_filter::EventFilter;
use crate::streamer::Streamer;

pub const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub struct EventHandler<ES: EventStore> {
    module_cache: SyncModuleCache<ResolverWrapper<AuthorityStore>>,
    event_streamer: Streamer<EventEnvelope, EventFilter>,
    pub(crate) event_store: Arc<ES>,
}

impl<ES: EventStore> EventHandler<ES> {
    pub fn new(validator_store: Arc<AuthorityStore>, event_store: Arc<ES>) -> Self {
        let streamer = Streamer::spawn(EVENT_DISPATCH_BUFFER_SIZE);
        Self {
            module_cache: SyncModuleCache::new(ResolverWrapper(validator_store)),
            event_streamer: streamer,
            event_store,
        }
    }

    // TODO: feed in current checkpoint number
    pub async fn process_events(
        &self,
        effects: &TransactionEffects,
        timestamp_ms: u64,
        checkpoint_num: u64,
    ) -> SuiResult {
        // serially dispatch event processing to honor events' orders.
        let res: Result<Vec<_>, _> = effects
            .events
            .iter()
            .map(|e| self.create_envelope(e, effects.transaction_digest, timestamp_ms))
            .collect();
        let envelopes = res?;

        // Ingest all envelopes together at once (for efficiency) into Event Store
        // It's good to ingest into store first before sending so that any failures in sending could
        // use the store as a backing for reliability
        self.event_store
            .add_events(&envelopes, checkpoint_num)
            .await?;
        debug!(
            num_events = envelopes.len(),
            checkpoint_num, "Finished writing events to event store"
        );

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
        timestamp_ms: u64,
    ) -> Result<EventEnvelope, SuiError> {
        let json_value = match event {
            Event::MoveEvent(event_obj) => {
                debug!(event =? event, "Process MoveEvent.");
                let move_struct = event_obj.to_move_struct_with_resolver(
                    ObjectFormatOptions::default(),
                    &self.module_cache,
                )?;
                Some(serde_json::to_value(&move_struct).map_err(|e| {
                    SuiError::ObjectSerializationError {
                        error: e.to_string(),
                    }
                })?)
            }
            _ => None,
        };

        Ok(EventEnvelope::new(
            timestamp_ms,
            Some(digest),
            event.clone(),
            json_value,
        ))
    }

    pub fn subscribe(&self, filter: EventFilter) -> impl Stream<Item = EventEnvelope> {
        self.event_streamer.subscribe(filter)
    }
}
