// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::streamer::Streamer;
use move_bytecode_utils::module_cache::SyncModuleCache;
use std::sync::Arc;
use sui_types::{
    error::{SuiError, SuiResult},
    event::{Event, EventEnvelope},
    messages::TransactionEffects,
};
use tokio::sync::mpsc::{self, Sender};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{debug, error};

pub const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub struct EventHandler {
    module_cache: SyncModuleCache<ResolverWrapper<AuthorityStore>>,
    streamer_queue: Sender<EventEnvelope>,
    streamer: Streamer,
}

impl EventHandler {
    pub fn new(validator_store: Arc<AuthorityStore>) -> Self {
        let (tx, rx) = mpsc::channel::<EventEnvelope>(EVENT_DISPATCH_BUFFER_SIZE);
        let streamer = Streamer::spawn(rx);
        Self {
            module_cache: SyncModuleCache::new(ResolverWrapper(validator_store)),
            streamer_queue: tx,
            streamer,
        }
    }

    pub async fn process_events(&self, effects: &TransactionEffects, timestamp_ms: u64) {
        // serializely dispatch event processing to honor events' orders.
        for event in &effects.events {
            if let Err(e) = self.process_event(event, timestamp_ms).await {
                error!(error =? e, "Failed to send EventEnvelope to dispatch");
            }
        }
    }

    pub async fn process_event(&self, event: &Event, timestamp_ms: u64) -> SuiResult {
        let envelope = match event {
            Event::MoveEvent { .. } => {
                debug!(event =? event, "Process MoveEvent.");
                match event.extract_move_struct(&self.module_cache) {
                    Ok(Some(move_struct)) => {
                        let json_value = serde_json::to_value(&move_struct).map_err(|e| {
                            SuiError::ObjectSerializationError {
                                error: e.to_string(),
                            }
                        })?;
                        EventEnvelope::new(timestamp_ms, None, event.clone(), Some(json_value))
                    }
                    Ok(None) => unreachable!("Expect a MoveStruct from a MoveEvent."),
                    Err(e) => return Err(e),
                }
            }
            _ => EventEnvelope::new(timestamp_ms, None, event.clone(), None),
        };

        // TODO store events here

        self.streamer_queue
            .send(envelope)
            .await
            .map_err(|e| SuiError::EventFailedToDispatch {
                error: e.to_string(),
            })
    }

    pub fn subscribe(&self) -> BroadcastStream<EventEnvelope> {
        self.streamer.subscribe()
    }
}
