// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_bytecode_utils::module_cache::SyncModuleCache;
use sui_json_rpc_api::rpc_types::SuiMoveStruct;
use tokio_stream::Stream;
use tracing::{debug, error};

use sui_types::object::{MoveObject, ObjectFormatOptions};
use sui_types::{
    error::{SuiError, SuiResult},
    event::{Event, EventEnvelope},
    messages::TransactionEffects,
};

use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::streamer::Streamer;
use sui_types::event_filter::EventFilter;

pub const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub struct EventHandler {
    module_cache: SyncModuleCache<ResolverWrapper<AuthorityStore>>,
    event_streamer: Streamer<EventEnvelope, EventFilter>,
}

impl EventHandler {
    pub fn new(validator_store: Arc<AuthorityStore>) -> Self {
        let streamer = Streamer::spawn(EVENT_DISPATCH_BUFFER_SIZE);
        Self {
            module_cache: SyncModuleCache::new(ResolverWrapper(validator_store)),
            event_streamer: streamer,
        }
    }

    pub async fn process_events(&self, effects: &TransactionEffects, timestamp_ms: u64) {
        // serially dispatch event processing to honor events' orders.
        for event in &effects.events {
            if let Err(e) = self.process_event(event, timestamp_ms).await {
                error!(error =? e, "Failed to send EventEnvelope to dispatch");
            }
        }
    }

    pub async fn process_event(&self, event: &Event, timestamp_ms: u64) -> SuiResult {
        let json_value = match event {
            Event::MoveEvent {
                type_, contents, ..
            } => {
                debug!(event =? event, "Process MoveEvent.");
                let move_object = MoveObject::new(type_.clone(), contents.clone());
                // Convert into `SuiMoveStruct` which is a mirror of MoveStruct but will additional type supports, (e.g. ascii::String).
                let move_struct: SuiMoveStruct = move_object
                    .to_move_struct_with_resolver(
                        ObjectFormatOptions::default(),
                        &self.module_cache,
                    )?
                    .into();
                Some(serde_json::to_value(&move_struct).map_err(|e| {
                    SuiError::ObjectSerializationError {
                        error: e.to_string(),
                    }
                })?)
            }
            _ => None,
        };
        let envelope = EventEnvelope::new(timestamp_ms, None, event.clone(), json_value);
        // TODO store events here
        self.event_streamer.send(envelope).await
    }

    pub fn subscribe(&self, filter: EventFilter) -> impl Stream<Item = EventEnvelope> {
        self.event_streamer.subscribe(filter)
    }
}
