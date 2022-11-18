// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! EventStore API supports high velocity event ingestion plus flexible event querying.
//! Multiple use cases supported:
//! - Explorer reads of different events
//! - Filtering of events per Move package, type, or other fields
//! - Persistent/reliable streaming, which needs to recover filtered events from a marker
//!   or point in time
//!
//! Events are also archived into checkpoints so this API should support that as well.
//!

use std::collections::BTreeMap;
use std::str::FromStr;
use std::usize;

use anyhow::anyhow;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use flexstr::SharedStr;
use futures::prelude::stream::BoxStream;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use move_core_types::value::MoveValue;
use serde_json::Value;
use tokio_stream::StreamExt;

pub use sql::SqlEventStore;
use sui_json_rpc_types::{SuiEvent, SuiEventEnvelope};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::error::SuiError;
use sui_types::error::SuiError::{StorageCorruptedFieldError, StorageMissingFieldError};
use sui_types::event::{BalanceChangeType, Event, EventID};
use sui_types::event::{EventEnvelope, EventType};
use sui_types::object::Owner;

pub mod sql;
pub mod test_utils;

/// Maximum number of events one can ask for right now
pub const EVENT_STORE_QUERY_MAX_LIMIT: usize = 1000;

pub const OBJECT_VERSION_KEY: &str = "obj_ver";
pub const AMOUNT_KEY: &str = "amount";
pub const BALANCE_CHANGE_TYPE_KEY: &str = "change_type";

/// One event pulled out from the EventStore
#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEvent {
    id: EventID,
    /// UTC timestamp in milliseconds
    timestamp: u64,
    /// Not present for non-transaction System events (eg EpochChange)
    tx_digest: Option<TransactionDigest>,
    /// The variant name from SuiEvent, eg MoveEvent, Publish, etc.
    event_type: SharedStr,
    /// Package ID if available
    package_id: Option<ObjectID>,
    /// Module name of the Move package generating the event
    module_name: Option<SharedStr>,
    /// Function name that produced the event, for Move Events
    function_name: Option<SharedStr>,
    /// Object Type
    object_type: Option<String>,
    /// Object ID of NewObject, DeleteObject, package being published, or object being transferred
    object_id: Option<ObjectID>,
    /// Individual event fields.  As much as possible these should be deconstructed and flattened,
    /// ie `{'obj': {'fieldA': 'A', 'fieldB': 'B'}}` should really be broken down to
    /// `[('obj.fieldA', 'A'), ('obj.fieldB', 'B')]
    ///
    /// There is no guarantee of ordering in the fields.
    ///
    /// ## Common field names
    /// * `version` - used by TransferObject
    /// * `destination` - address, in hex bytes, used by TransferObject
    /// * `type` - used by TransferObject (TransferType - Coin, ToAddress, ToObject)
    fields: BTreeMap<SharedStr, EventValue>, // Change this to something based on CBOR for binary values, or our own value types for efficiency
    /// Contents for MoveEvent
    move_event_contents: Option<Vec<u8>>,
    /// StructTag in string form for MoveEvent, e.g. "0x2::devnet_nft::MintNFTEvent"
    move_event_name: Option<String>,
    /// Sender in the event
    sender: Option<SuiAddress>,
    /// Recipient in the event
    recipient: Option<Owner>,
}

impl StoredEvent {
    pub fn into_move_event(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let type_ = self.move_event_name.as_ref().ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing move_event_name for event {:?}",
                self
            )))
        })?;
        if self.move_event_contents.is_none() {
            anyhow::bail!(StorageMissingFieldError(format!(
                "Missing move_event_contents for event {:?}",
                self
            )))
        }
        // Safe to unwrap as we checked it nullability above
        let bcs = self.move_event_contents.unwrap();
        Ok(SuiEvent::MoveEvent {
            package_id,
            transaction_module,
            sender,
            type_: type_.clone(),
            fields: None,
            bcs,
        })
    }

    pub fn into_publish(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let sender = self.sender()?;
        Ok(SuiEvent::Publish { sender, package_id })
    }

    pub fn into_transfer_object(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let recipient = self.recipient()?;
        let object_id = self.object_id()?;
        let object_type = self.object_type()?;
        let version = self.object_version()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract object version from StoredEvent: {self:?}")
        })?;
        Ok(SuiEvent::TransferObject {
            package_id,
            transaction_module,
            sender,
            recipient,
            object_type,
            object_id,
            version,
        })
    }

    pub fn into_mutate_object(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let object_id = self.object_id()?;
        let object_type = self.object_type()?;
        let version = self.object_version()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract object version from StoredEvent: {self:?}")
        })?;
        Ok(SuiEvent::MutateObject {
            package_id,
            transaction_module,
            sender,
            object_type,
            object_id,
            version,
        })
    }

    pub fn into_delete_object(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let object_id = self.object_id()?;
        let version = self.object_version()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract object version from StoredEvent: {self:?}")
        })?;
        Ok(SuiEvent::DeleteObject {
            package_id,
            transaction_module,
            sender,
            object_id,
            version,
        })
    }

    pub fn into_new_object(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let object_id = self.object_id()?;
        let recipient = self.recipient()?;
        let object_type = self.object_type()?;
        let version = self.object_version()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract object version from StoredEvent: {self:?}")
        })?;
        Ok(SuiEvent::NewObject {
            package_id,
            transaction_module,
            sender,
            recipient,
            object_type,
            object_id,
            version,
        })
    }

    pub fn into_coin_balance_change(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let owner = self.recipient()?;
        let coin_type = self.object_type()?;
        let coin_object_id = self.object_id()?;
        let change_type = self.change_type()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract balance change type from StoredEvent: {self:?}")
        })?;
        let version = self.object_version()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract object version from StoredEvent: {self:?}")
        })?;
        let amount = self
            .amount()?
            .ok_or_else(|| anyhow::anyhow!("Can't extract amount from StoredEvent: {self:?}"))?;

        Ok(SuiEvent::CoinBalanceChange {
            package_id,
            transaction_module,
            sender,
            change_type,
            owner,
            coin_type,
            coin_object_id,
            version,
            amount,
        })
    }

    /// Convert a vec of StoredEvents into a vec of SuiEventEnvelope.
    /// Returns Err when any conversion fails.
    pub fn into_event_envelopes(
        stored_events: Vec<Self>,
    ) -> Result<Vec<(EventID, SuiEventEnvelope)>, anyhow::Error> {
        let mut events = Vec::with_capacity(stored_events.len());
        for stored_event in stored_events {
            let id = stored_event.id.clone();
            let event_envelope: SuiEventEnvelope = stored_event.try_into()?;
            events.push((id, event_envelope));
        }
        Ok(events)
    }

    fn package_id(&self) -> Result<ObjectID, anyhow::Error> {
        self.package_id.ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing package_id for event {:?}",
                self
            )))
        })
    }

    fn transaction_module(&self) -> Result<String, anyhow::Error> {
        let module_name = self.module_name.as_ref().ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing transaction_module for event {:?}",
                self
            )))
        })?;
        Ok(Identifier::from_str(module_name.as_str())
            .map_err(|e| {
                anyhow!(StorageCorruptedFieldError(format!(
                    "Module identifier is invalid for event {:?}: {e:?}",
                    self
                )))
            })?
            .to_string())
    }

    fn sender(&self) -> Result<SuiAddress, anyhow::Error> {
        self.sender.ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing sender for event {:?}",
                self
            )))
        })
    }

    fn recipient(&self) -> Result<Owner, anyhow::Error> {
        self.recipient.ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing recipient for event {:?}",
                self
            )))
        })
    }

    fn object_type(&self) -> Result<String, anyhow::Error> {
        self.object_type.clone().ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing object_type for event {:?}",
                self
            )))
        })
    }

    fn object_id(&self) -> Result<ObjectID, anyhow::Error> {
        self.object_id.ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing object_id for event {:?}",
                self
            )))
        })
    }

    fn extract_string_field(&self, key: &str) -> Result<Option<String>, anyhow::Error> {
        let field_value = self.fields.get(key);
        match field_value {
            Some(EventValue::Json(serde_json::Value::String(string))) => Ok(Some(string.clone())),
            None => Ok(None),
            Some(other_value) => anyhow::bail!(SuiError::ExtraFieldFailedToDeserialize {
                error: format!("Got unexpected stored value for {key}: {other_value:?}"),
            }),
        }
    }

    fn object_version(&self) -> Result<Option<SequenceNumber>, anyhow::Error> {
        self.extract_string_field(OBJECT_VERSION_KEY).map(|opt| {
            opt.and_then(|s| u64::from_str(&s).ok())
                .map(SequenceNumber::from_u64)
        })
    }

    fn change_type(&self) -> Result<Option<BalanceChangeType>, anyhow::Error> {
        Ok(self
            .extract_string_field(BALANCE_CHANGE_TYPE_KEY)?
            .map(|opt| usize::from_str(&opt))
            .transpose()?
            .map(Event::balance_change_from_ordinal)
            .transpose()?)
    }

    fn amount(&self) -> Result<Option<i128>, anyhow::Error> {
        self.extract_string_field(AMOUNT_KEY)
            .map(|opt| opt.and_then(|s| i128::from_str(&s).ok()))
    }
}

impl TryInto<SuiEventEnvelope> for StoredEvent {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<SuiEventEnvelope, Self::Error> {
        let timestamp = self.timestamp;
        let tx_digest = self.tx_digest;
        let event_type_str = self.event_type.as_str();
        let event_id = self.id.clone();
        let event = match EventType::from_str(event_type_str) {
            Ok(type_) => {
                match type_ {
                    EventType::MoveEvent => self.into_move_event(),
                    EventType::Publish => self.into_publish(),
                    EventType::TransferObject => self.into_transfer_object(),
                    EventType::MutateObject => self.into_mutate_object(),
                    EventType::DeleteObject => self.into_delete_object(),
                    EventType::NewObject => self.into_new_object(),
                    EventType::CoinBalanceChange => self.into_coin_balance_change(),
                    // TODO support "EpochChange" and "Checkpoint"
                    EventType::EpochChange => anyhow::bail!("Unsupported event type: EpochChange"),
                    EventType::Checkpoint => anyhow::bail!("Unsupported event type: Checkpoint"),
                }
            }
            Err(e) => anyhow::bail!("Invalid EventType {event_type_str}: {e:?}"),
        }?;
        Ok(SuiEventEnvelope {
            timestamp,
            tx_digest,
            id: event_id,
            event,
        })
    }
}

/// Enum for different types of values returnable from events in the EventStore
// This is distinct from MoveValue because we want to explicitly represent (and translate)
// blobs and strings, allowing us to use more efficient representations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventValue {
    Move(MoveValue),
    /// Efficient string representation, no allocation for small strings
    String(SharedStr),
    /// Arbitrary-length blob.  Please use MoveValue::Address for ObjectIDs and similar things.
    BinaryBlob(Vec<u8>),
    Json(Value),
}

/// An EventStore supports event ingestion and flexible event querying
/// One can think of events as logs.  They represent a log of what is happening to Sui.
/// Thus, all different kinds of events fit on a timeline, and one should be able to query for
/// different types of events that happen over that timeline.
#[async_trait]
#[enum_dispatch]
pub trait EventStore {
    /// Adds a batch of transaction-related events to the EventStore.
    /// Semantics:
    /// - The batch is appended to the store.
    /// - The batch may contain events from multiple transactions.
    /// - Each event must have a unique (seq_num, event_num) tuple - events that duplicate this key
    /// will be ignored.
    ///
    /// Returns Ok(rows_affected).
    async fn add_events(&self, events: &[EventEnvelope]) -> Result<u64, SuiError>;

    /// Returns at most `limit` events emitted by all transaction, ordered .
    async fn all_events(
        &self,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;
    /// Returns at most `limit` events emitted by a given
    /// transaction, sorted in time order defined by the [descending] parameter.
    async fn events_by_transaction(
        &self,
        digest: TransactionDigest,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events of a certain EventType
    /// (e.g. `TransferObject`) within [start_time, end_time),
    /// sorted in time order defined by the [descending] parameter.
    async fn events_by_type(
        &self,
        event_type: EventType,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events emitted in a certain Module ID during
    /// sorted in time order defined by the [descending] parameter.
    async fn events_by_module_id(
        &self,
        module: &ModuleId,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events with the move event struct name
    /// (e.g. `0x2::devnet_nft::MintNFTEvent`) emitted
    /// sorted in time order defined by the [descending] parameter.
    /// For ease of use, move_event_struct_name may be a prefix, so that for example only
    /// `0x2::devnet_nft` is needed to find all move events in the devnet_nft module.
    async fn events_by_move_event_struct_name(
        &self,
        move_event_struct_name: &str,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events associated with a certain sender
    /// sorted in time order defined by the [descending] parameter.
    async fn events_by_sender(
        &self,
        sender: &SuiAddress,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events associated with a certain recipient
    /// sorted in time order defined by the [descending] parameter.
    async fn events_by_recipient(
        &self,
        recipient: &Owner,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events associated with a certain object id
    /// sorted in time order defined by the [descending] parameter.
    async fn events_by_object(
        &self,
        object: &ObjectID,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Generic event iterator that returns events emitted between
    /// [start_time, end_time), sorted in time order defined by the [descending] parameter.
    async fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
        cursor: EventID,
        limit: usize,
        descending: bool,
    ) -> Result<Vec<StoredEvent>, SuiError>;
}

/// EventStoreType contains different implementations of EventStores, but implements the EventStore trait.
/// It allows fast inlineable static calls without needing generics.
#[enum_dispatch(EventStore)]
pub enum EventStoreType {
    SqlEventStore,
}

/// A wrapper around streaming results which makes them easier to deal with
// TODO: make it generic for non events
pub struct StreamedResult<'s> {
    inner: BoxStream<'s, Result<StoredEvent, SuiError>>,
}

impl<'s> StreamedResult<'s> {
    pub fn new(stream: BoxStream<'s, Result<StoredEvent, SuiError>>) -> Self {
        Self { inner: stream }
    }

    /// Pulls out a chunk of up to max_items items
    /// NOTE: if there are no more items in the stream, then empty chunk is returned
    pub async fn next_chunk(&mut self, max_items: usize) -> Result<Vec<StoredEvent>, SuiError> {
        let mut items = Vec::new();
        while let Some(res) = self.inner.next().await {
            match res {
                Err(e) => return Err(e),
                Ok(event) if items.len() < max_items => items.push(event),
                _ => break,
            }
        }
        Ok(items)
    }
}
