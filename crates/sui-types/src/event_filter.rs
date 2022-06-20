// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use serde_json::Value;

use crate::base_types::SuiAddress;
use crate::event::{Event, EventEnvelope};
use crate::event::{EventType, TransferType};
use crate::object::Owner;
use crate::ObjectID;

#[cfg(test)]
#[path = "unit_tests/event_filter_tests.rs"]
mod event_filter_tests;

#[derive(Clone, Debug)]
pub enum EventFilter {
    Package(ObjectID),
    Module(Identifier),
    Function(Identifier),
    MoveEventType(StructTag),
    EventType(EventType),
    MoveEventField { path: String, value: Value },
    InstigatorAddress(SuiAddress),
    Recipient(Owner),
    ObjectId(ObjectID),
    TransferType(TransferType),
    MatchAll(Vec<EventFilter>),
    MatchAny(Vec<EventFilter>),
}
impl EventFilter {
    fn try_matches(&self, item: &EventEnvelope) -> Result<bool, anyhow::Error> {
        Ok(match self {
            EventFilter::MoveEventType(event_type) => match &item.event {
                Event::MoveEvent { type_, .. } => type_ == event_type,
                _ => false,
            },
            EventFilter::MoveEventField { path, value } => match &item.move_struct_json_value {
                Some(json) => {
                    matches!(json.pointer(path), Some(v) if v == value)
                }
                _ => false,
            },
            EventFilter::InstigatorAddress(sender) => {
                matches!(&item.event.sender(), Some(addr) if addr == sender)
            }
            EventFilter::Package(obj_id) => {
                matches!(&item.event.package_id(), Some(id) if id == obj_id)
            }
            EventFilter::Module(module) => {
                matches!(item.event.module_name(), Some(name) if name == module.as_str())
            }
            EventFilter::Function(function) => {
                matches!(item.event.function_name(), Some(name) if name == function.as_str())
            }
            EventFilter::ObjectId(object_id) => {
                matches!(item.event.object_id(), Some(id) if &id == object_id)
            }
            EventFilter::EventType(type_) => &item.event.event_type() == type_,
            EventFilter::MatchAll(filters) => filters.iter().all(|f| f.matches(item)),
            EventFilter::MatchAny(filters) => filters.iter().any(|f| f.matches(item)),
            EventFilter::TransferType(type_) => {
                matches!(item.event.transfer_type(), Some(transfer_type) if transfer_type == type_)
            }
            EventFilter::Recipient(recipient) => {
                matches!(item.event.recipient(), Some(event_recipient) if event_recipient == recipient)
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
