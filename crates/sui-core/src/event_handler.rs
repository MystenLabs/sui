// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{AuthorityStore, AuthorityStoreWrapper};
use crate::streamer::Streamer;
use chrono::prelude::*;
use move_bytecode_utils::module_cache::SyncModuleCache;
use std::convert::TryFrom;
use std::sync::Arc;
use sui_types::{
    error::{SuiError, SuiResult},
    event::{Event, EventEnvelope},
    messages::TransactionEffects,
};
use tokio::sync::mpsc::{self, Sender};
use tracing::{debug, error};

const EVENT_DISPATCH_BUFFER_SIZE: usize = 1000;

pub fn get_unixtime_ms() -> u64 {
    let ts_ms = Utc::now().timestamp_millis();
    u64::try_from(ts_ms).expect("Travelling in time machine")
}

pub struct EventHandler {
    module_cache: SyncModuleCache<AuthorityStoreWrapper>,
    streamer_queue: Sender<EventEnvelope>,
}

impl EventHandler {
    pub fn new(validator_store: Arc<AuthorityStore>) -> Self {
        let (tx, rx) = mpsc::channel::<EventEnvelope>(EVENT_DISPATCH_BUFFER_SIZE);
        Streamer::spawn(rx);
        Self {
            module_cache: SyncModuleCache::new(AuthorityStoreWrapper(validator_store)),
            streamer_queue: tx,
        }
    }

    pub async fn process_events(&self, effects: &TransactionEffects) {
        // serializely dispatch event processing to honor events' orders.
        for event in &effects.events {
            if let Err(e) = self.process_event(event).await {
                error!(error =? e, "Failed to send EventEnvolope to dispatch");
            }
        }
    }

    pub async fn process_event(&self, event: &Event) -> SuiResult {
        let envolope = match event {
            Event::MoveEvent { .. } => {
                debug!(event =? event, "Process MoveEvent.");
                match event.extract_move_struct(&self.module_cache) {
                    Ok(Some(move_struct)) => EventEnvelope::new(
                        get_unixtime_ms(),
                        None,
                        event.clone(),
                        Some(move_struct),
                    ),
                    Ok(None) => unreachable!("Expect a MoveStruct from a MoveEvent."),
                    Err(e) => return Err(e),
                }
            }
            _ => EventEnvelope::new(get_unixtime_ms(), None, event.clone(), None),
        };

        // TODO store events here

        self.streamer_queue
            .send(envolope)
            .await
            .map_err(|e| SuiError::EventFailedToDispatch {
                error: e.to_string(),
            })
    }
}
