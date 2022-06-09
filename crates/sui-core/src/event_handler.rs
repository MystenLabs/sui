// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::streamer::Streamer;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::sync::Arc;

use chrono::prelude::*;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::parser::parse_struct_tag;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;
use tokio_stream::Stream;
use tracing::{debug, error};

use sui_types::object::ObjectFormatOptions;
use sui_types::{
    error::{SuiError, SuiResult},
    event::{Event, EventEnvelope},
    messages::TransactionEffects,
};

use crate::authority::{AuthorityStore, ResolverWrapper};
use crate::streamer::{Filter, Streamer};

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
        let envelope = EventEnvelope::new(timestamp_ms, None, event.clone(), json_value);
        // TODO store events here
        self.event_streamer.send(envelope).await
    }

    pub fn subscribe(&self, filter: EventFilter) -> impl Stream<Item = EventEnvelope> {
        self.event_streamer.subscribe(filter)
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct EventFieldFilter {
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum EventFilter {
    ByContract(String, Option<String>, Option<String>, EventFieldFilter),
    ByType(String, EventFieldFilter),
    ByAddress,
}
// TODO: implement other filter types
impl EventFilter {
    fn try_matches(&self, item: &EventEnvelope) -> Result<bool, anyhow::Error> {
        Ok(match self {
            EventFilter::ByType(event_type, fields_filter) => match &item.event {
                Event::MoveEvent(event_obj) => {
                    let event_type = parse_struct_tag(event_type)?;
                    if let Some(json) = &item.move_struct_json_value {
                        for (pointer, value) in &fields_filter.fields {
                            if let Some(v) = json.pointer(pointer) {
                                if &v.to_string() != value {
                                    return Ok(false);
                                }
                            } else {
                                return Ok(false);
                            }
                        }
                    }
                    event_obj.type_ == event_type
                }
                _ => false,
            },
            EventFilter::ByContract(..) => true,
            EventFilter::ByAddress => true,
        })
    }
}

impl Filter<EventEnvelope> for EventFilter {
    fn matches(&self, item: &EventEnvelope) -> bool {
        self.try_matches(item).unwrap_or_default()
    }
}
