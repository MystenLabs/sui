// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base58;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::DisplayFromStr;

use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::error::SuiResult;
use sui_types::event::{Event, EventEnvelope, EventID};

use crate::{type_and_fields_from_move_struct, Page};

pub type EventPage = Page<SuiEvent, EventID>;

#[serde_as]
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "Event", rename_all = "camelCase")]
pub struct SuiEvent {
    /// Sequential event ID, ie (transaction seq number, event seq number).
    /// 1) Serves as a unique event ID for each fullnode
    /// 2) Also serves to sequence events for the purposes of pagination and querying.
    ///    A higher id is an event seen later by that fullnode.
    /// This ID is the "cursor" for event querying.
    pub id: EventID,
    /// Move package where this event was emitted.
    pub package_id: ObjectID,
    #[schemars(with = "String")]
    #[serde_as(as = "DisplayFromStr")]
    /// Move module where this event was emitted.
    pub transaction_module: Identifier,
    /// Sender's Sui address.
    pub sender: SuiAddress,
    #[schemars(with = "String")]
    #[serde_as(as = "DisplayFromStr")]
    /// Move event type.
    pub type_: StructTag,
    /// Parsed json value of the event
    pub parsed_json: Value,
    #[serde_as(as = "Base58")]
    #[schemars(with = "Base58")]
    /// Base 58 encoded bcs bytes of the move event
    pub bcs: Vec<u8>,
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_ms: Option<u64>,
}

impl From<EventEnvelope> for SuiEvent {
    fn from(ev: EventEnvelope) -> Self {
        Self {
            id: EventID {
                tx_digest: ev.tx_digest,
                event_seq: ev.event_num,
            },
            package_id: ev.event.package_id,
            transaction_module: ev.event.transaction_module,
            sender: ev.event.sender,
            type_: ev.event.type_,
            parsed_json: ev.parsed_json,
            bcs: ev.event.contents,
            timestamp_ms: Some(ev.timestamp),
        }
    }
}

impl SuiEvent {
    pub fn try_from(
        event: Event,
        tx_digest: TransactionDigest,
        event_seq: u64,
        timestamp_ms: Option<u64>,
        resolver: &impl GetModule,
    ) -> SuiResult<Self> {
        let Event {
            package_id,
            transaction_module,
            sender,
            type_,
            contents,
        } = event;

        let bcs = contents.to_vec();

        let move_struct = Event::move_event_to_move_struct(&type_, &contents, resolver)?;
        let (type_, field) = type_and_fields_from_move_struct(&type_, move_struct);

        Ok(SuiEvent {
            id: EventID {
                tx_digest,
                event_seq,
            },
            package_id,
            transaction_module,
            sender,
            type_,
            parsed_json: field.to_json_value(),
            bcs,
            timestamp_ms,
        })
    }
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum EventFilter {
    Package(ObjectID),
    Module(
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        Identifier,
    ),
    MoveEventType(
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        StructTag,
    ),
    MoveEventField {
        path: String,
        value: Value,
    },
    SenderAddress(SuiAddress),

    All(Vec<EventFilter>),
    Any(Vec<EventFilter>),
    And(Box<EventFilter>, Box<EventFilter>),
    Or(Box<EventFilter>, Box<EventFilter>),
}

impl EventFilter {
    fn try_matches(&self, item: &SuiEvent) -> SuiResult<bool> {
        Ok(match self {
            EventFilter::MoveEventType(event_type) => &item.type_ == event_type,
            EventFilter::MoveEventField { path, value } => {
                matches!(item.parsed_json.pointer(path), Some(v) if v == value)
            }
            EventFilter::SenderAddress(sender) => &item.sender == sender,
            EventFilter::Package(object_id) => &item.package_id == object_id,
            EventFilter::Module(module) => &item.transaction_module == module,
            EventFilter::All(filters) => filters.iter().all(|f| f.matches(item)),
            EventFilter::Any(filters) => filters.iter().any(|f| f.matches(item)),
            EventFilter::And(f1, f2) => {
                EventFilter::All(vec![*(*f1).clone(), *(*f2).clone()]).matches(item)
            }
            EventFilter::Or(f1, f2) => {
                EventFilter::Any(vec![*(*f1).clone(), *(*f2).clone()]).matches(item)
            }
        })
    }

    pub fn and(self, other_filter: EventFilter) -> Self {
        Self::All(vec![self, other_filter])
    }
    pub fn or(self, other_filter: EventFilter) -> Self {
        Self::Any(vec![self, other_filter])
    }
}

impl Filter<SuiEvent> for EventFilter {
    fn matches(&self, item: &SuiEvent) -> bool {
        self.try_matches(item).unwrap_or_default()
    }
}

pub trait Filter<T> {
    fn matches(&self, item: &T) -> bool;
}
