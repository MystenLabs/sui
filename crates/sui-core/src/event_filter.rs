// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use serde_json::Value;
use std::collections::BTreeMap;
use sui_types::base_types::SuiAddress;
use sui_types::event::{Event, EventEnvelope};

#[derive(Clone, Debug)]
pub enum EventFilter {
    ByPackage(AccountAddress),
    ByModule(Identifier),
    ByFunction(Identifier),
    ByMoveEventType(StructTag),
    ByMoveEventFields(BTreeMap<String, Value>),
    BySenderAddress(SuiAddress),
    ObjectId(SuiAddress),
    MatchAll(Vec<EventFilter>),
    MatchAny(Vec<EventFilter>),
}
impl EventFilter {
    fn try_matches(&self, item: &EventEnvelope) -> Result<bool, anyhow::Error> {
        Ok(match self {
            EventFilter::ByMoveEventType(event_type) => match &item.event {
                Event::MoveEvent(event_obj) => &event_obj.type_ == event_type,
                // TODO: impl for non-move event
                _ => false,
            },
            EventFilter::ByMoveEventFields(fields_filter) => {
                if let Some(json) = &item.move_struct_json_value {
                    for (pointer, value) in fields_filter {
                        if let Some(v) = json.pointer(pointer) {
                            if v != value {
                                return Ok(false);
                            }
                        } else {
                            return Ok(false);
                        }
                    }
                    true
                } else {
                    false
                }
            }
            // TODO: Implement the rest
            EventFilter::BySenderAddress(_) => true,
            EventFilter::ByPackage(_) => true,
            EventFilter::ByModule(_) => true,
            EventFilter::ByFunction(_) => true,
            EventFilter::ObjectId(_) => true,

            EventFilter::MatchAll(filters) => {
                for filter in filters {
                    if !filter.matches(item) {
                        return Ok(false);
                    }
                }
                true
            }
            EventFilter::MatchAny(filters) => {
                for filter in filters {
                    if filter.matches(item) {
                        return Ok(true);
                    }
                }
                false
            }
        })
    }

    pub fn and(self, other_filter: EventFilter) -> Self {
        Self::MatchAll(vec![self, other_filter])
    }
}

impl Filter<EventEnvelope> for EventFilter {
    fn matches(&self, item: &EventEnvelope) -> bool {
        self.try_matches(item).unwrap_or_default()
    }
}

pub trait Filter<T> {
    fn matches(&self, item: &T) -> bool;
}
