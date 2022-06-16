// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SuiAddress;
use crate::event::EventType;
use crate::event::{Event, EventEnvelope};
use crate::ObjectID;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum EventFilter {
    ByPackage(ObjectID),
    ByModule(Identifier),
    ByFunction(Identifier),
    ByMoveEventType(StructTag),
    ByEventType(EventType),
    ByMoveEventFields(BTreeMap<String, Value>),
    BySenderAddress(SuiAddress),
    ObjectId(ObjectID),
    MatchAll(Vec<EventFilter>),
    MatchAny(Vec<EventFilter>),
}
impl EventFilter {
    fn try_matches(&self, item: &EventEnvelope) -> Result<bool, anyhow::Error> {
        Ok(match self {
            EventFilter::ByMoveEventType(event_type) => match &item.event {
                Event::MoveEvent(event_obj) => &event_obj.type_ == event_type,
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
            EventFilter::BySenderAddress(sender) => {
                matches!(&item.event.sender(), Some(addr) if addr == sender)
            }
            EventFilter::ByPackage(obj_id) => {
                matches!(&item.event.package_id(), Some(id) if id == obj_id)
            }
            EventFilter::ByModule(module) => {
                matches!(item.event.module_name(), Some(name) if name == module.as_str())
            }
            EventFilter::ByFunction(function) => {
                matches!(item.event.function_name(), Some(name) if name == function.as_str())
            }
            EventFilter::ObjectId(_) => true,
            EventFilter::ByEventType(type_) => &item.event.event_type() == type_,
            EventFilter::MatchAll(filters) => filters.iter().all(|f| f.matches(item)),
            EventFilter::MatchAny(filters) => filters.iter().any(|f| f.matches(item)),
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
