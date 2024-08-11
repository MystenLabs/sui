// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::{Base58, Base64};
use move_core_types::annotated_value::MoveDatatypeLayout;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use mysten_metrics::monitored_scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use serde_with::{serde_as, DisplayFromStr};
use std::fmt;
use std::fmt::Display;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::error::SuiResult;
use sui_types::event::{Event, EventEnvelope, EventID};
use sui_types::sui_serde::BigInt;

use json_to_table::json_to_table;
use tabled::settings::Style as TableStyle;

use crate::{type_and_fields_from_move_event_data, Page};
use sui_types::sui_serde::SuiStructTag;

#[cfg(any(feature = "test-utils", test))]
use std::str::FromStr;

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
    #[serde_as(as = "SuiStructTag")]
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
    #[schemars(with = "Option<BigInt<u64>>")]
    #[serde_as(as = "Option<BigInt<u64>>")]
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

impl From<SuiEvent> for Event {
    fn from(val: SuiEvent) -> Self {
        Event {
            package_id: val.package_id,
            transaction_module: val.transaction_module,
            sender: val.sender,
            type_: val.type_,
            contents: val.bcs,
        }
    }
}

impl SuiEvent {
    pub fn try_from(
        event: Event,
        tx_digest: TransactionDigest,
        event_seq: u64,
        timestamp_ms: Option<u64>,
        layout: MoveDatatypeLayout,
    ) -> SuiResult<Self> {
        let Event {
            package_id,
            transaction_module,
            sender,
            type_: _,
            contents,
        } = event;

        let bcs = contents.to_vec();

        let move_value = Event::move_event_to_move_value(&contents, layout)?;
        let (type_, fields) = type_and_fields_from_move_event_data(move_value)?;

        Ok(SuiEvent {
            id: EventID {
                tx_digest,
                event_seq,
            },
            package_id,
            transaction_module,
            sender,
            type_,
            parsed_json: fields,
            bcs,
            timestamp_ms,
        })
    }
}

impl Display for SuiEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parsed_json = &mut self.parsed_json.clone();
        bytes_array_to_base64(parsed_json);
        let mut table = json_to_table(parsed_json);
        let style = TableStyle::modern();
        table.collapse().with(style);
        write!(f,
            " ┌──\n │ EventID: {}:{}\n │ PackageID: {}\n │ Transaction Module: {}\n │ Sender: {}\n │ EventType: {}\n",
            self.id.tx_digest, self.id.event_seq, self.package_id, self.transaction_module, self.sender, self.type_)?;
        if let Some(ts) = self.timestamp_ms {
            writeln!(f, " │ Timestamp: {}\n └──", ts)?;
        }
        writeln!(f, " │ ParsedJSON:")?;
        let table_string = table.to_string();
        let table_rows = table_string.split_inclusive('\n');
        for r in table_rows {
            write!(f, " │   {r}")?;
        }

        write!(f, "\n └──")
    }
}

#[cfg(any(feature = "test-utils", test))]
impl SuiEvent {
    pub fn random_for_testing() -> Self {
        Self {
            id: EventID {
                tx_digest: TransactionDigest::random(),
                event_seq: 0,
            },
            package_id: ObjectID::random(),
            transaction_module: Identifier::from_str("random_for_testing").unwrap(),
            sender: SuiAddress::random_for_testing_only(),
            type_: StructTag::from_str("0x6666::random_for_testing::RandomForTesting").unwrap(),
            parsed_json: json!({}),
            bcs: vec![],
            timestamp_ms: None,
        }
    }
}

/// Convert a json array of bytes to Base64
fn bytes_array_to_base64(v: &mut Value) {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => (),
        Value::Array(vals) => {
            if let Some(vals) = vals.iter().map(try_into_byte).collect::<Option<Vec<_>>>() {
                *v = json!(Base64::from_bytes(&vals).encoded())
            } else {
                for val in vals {
                    bytes_array_to_base64(val)
                }
            }
        }
        Value::Object(map) => {
            for val in map.values_mut() {
                bytes_array_to_base64(val)
            }
        }
    }
}

/// Try to convert a json Value object into an u8.
fn try_into_byte(v: &Value) -> Option<u8> {
    let num = v.as_u64()?;
    (num <= 255).then_some(num as u8)
}

#[serde_as]
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub enum EventFilter {
    /// Query by sender address.
    Sender(SuiAddress),
    /// Return events emitted by the given transaction.
    Transaction(
        ///digest of the transaction, as base-64 encoded string
        TransactionDigest,
    ),
    /// Return events emitted in a specified Package.
    Package(ObjectID),
    /// Return events emitted in a specified Move module.
    /// If the event is defined in Module A but emitted in a tx with Module B,
    /// query `MoveModule` by module B returns the event.
    /// Query `MoveEventModule` by module A returns the event too.
    MoveModule {
        /// the Move package ID
        package: ObjectID,
        /// the module name
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        module: Identifier,
    },
    /// Return events with the given Move event struct name (struct tag).
    /// For example, if the event is defined in `0xabcd::MyModule`, and named
    /// `Foo`, then the struct tag is `0xabcd::MyModule::Foo`.
    MoveEventType(
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        StructTag,
    ),
    /// Return events with the given Move module name where the event struct is defined.
    /// If the event is defined in Module A but emitted in a tx with Module B,
    /// query `MoveEventModule` by module A returns the event.
    /// Query `MoveModule` by module B returns the event too.
    MoveEventModule {
        /// the Move package ID
        package: ObjectID,
        /// the module name
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        module: Identifier,
    },
    MoveEventField {
        path: String,
        value: Value,
    },
    /// Return events emitted in [start_time, end_time] interval
    #[serde(rename_all = "camelCase")]
    TimeRange {
        /// left endpoint of time interval, milliseconds since epoch, inclusive
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        start_time: u64,
        /// right endpoint of time interval, milliseconds since epoch, exclusive
        #[schemars(with = "BigInt<u64>")]
        #[serde_as(as = "BigInt<u64>")]
        end_time: u64,
    },

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
            EventFilter::Sender(sender) => &item.sender == sender,
            EventFilter::Package(object_id) => &item.package_id == object_id,
            EventFilter::MoveModule { package, module } => {
                &item.transaction_module == module && &item.package_id == package
            }
            EventFilter::All(filters) => filters.iter().all(|f| f.matches(item)),
            EventFilter::Any(filters) => filters.iter().any(|f| f.matches(item)),
            EventFilter::And(f1, f2) => {
                EventFilter::All(vec![*(*f1).clone(), *(*f2).clone()]).matches(item)
            }
            EventFilter::Or(f1, f2) => {
                EventFilter::Any(vec![*(*f1).clone(), *(*f2).clone()]).matches(item)
            }
            EventFilter::Transaction(digest) => digest == &item.id.tx_digest,

            EventFilter::TimeRange {
                start_time,
                end_time,
            } => {
                if let Some(timestamp) = &item.timestamp_ms {
                    start_time <= timestamp && end_time > timestamp
                } else {
                    false
                }
            }
            EventFilter::MoveEventModule { package, module } => {
                &item.type_.module == module && &ObjectID::from(item.type_.address) == package
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
        let _scope = monitored_scope("EventFilter::matches");
        self.try_matches(item).unwrap_or_default()
    }
}

pub trait Filter<T> {
    fn matches(&self, item: &T) -> bool;
}
