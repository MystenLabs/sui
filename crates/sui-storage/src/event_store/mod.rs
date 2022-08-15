// Copyright (c) 2022, Mysten Labs, Inc.
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

use anyhow::anyhow;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use futures::prelude::stream::BoxStream;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use move_core_types::value::MoveValue;
use serde_json::Value;
use std::str::FromStr;
use sui_json_rpc_types::{SuiEvent, SuiEventEnvelope};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::error::SuiError;
use sui_types::error::SuiError::{StorageCorruptedFieldError, StorageMissingFieldError};
use sui_types::event::TransferType;
use sui_types::event::{EventEnvelope, EventType};
use sui_types::object::Owner;
use tokio_stream::StreamExt;

pub mod sql;
pub mod test_utils;
pub use sql::SqlEventStore;

use flexstr::SharedStr;

/// One event pulled out from the EventStore
#[allow(unused)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredEvent {
    /// UTC timestamp in milliseconds
    timestamp: u64,
    checkpoint_num: u64,
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
    fields: Vec<(SharedStr, EventValue)>, // Change this to something based on CBOR for binary values, or our own value types for efficiency
    /// Contents for MoveEvent
    move_event_contents: Option<Vec<u8>>,
    /// StructTag for MoveEvent
    move_event_struct_tag: Option<String>,
    /// Sender in the event
    /// FIXME - Owner
    sender: Option<SuiAddress>,
    /// Recipient in the event
    recipient: Option<Owner>,
    /// Sequence number of the mentioned object in event
    object_version: Option<SequenceNumber>,
    /// Transfer type of the event
    transfer_type: Option<TransferType>,
}

impl StoredEvent {
    pub fn into_move_event(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let type_ = self.move_event_struct_tag.as_ref().ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing move_event_struct_tag for event {:?}",
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
        let version = self.object_version()?;
        let type_ = self.transfer_type()?;
        Ok(SuiEvent::TransferObject {
            package_id,
            transaction_module,
            sender,
            recipient,
            object_id,
            version,
            type_: *type_,
        })
    }

    pub fn into_delete_object(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let object_id = self.object_id()?;
        Ok(SuiEvent::DeleteObject {
            package_id,
            transaction_module,
            sender,
            object_id,
        })
    }

    pub fn into_new_object(self) -> Result<SuiEvent, anyhow::Error> {
        let package_id = self.package_id()?;
        let transaction_module = self.transaction_module()?;
        let sender = self.sender()?;
        let object_id = self.object_id()?;
        let recipient = self.recipient()?;
        Ok(SuiEvent::NewObject {
            package_id,
            transaction_module,
            sender,
            recipient,
            object_id,
        })
    }

    /// Convert a vec of StoredEvents into a vec of SuiEventEnvelope.
    /// Returns Err when any conversion fails.
    pub fn into_event_envelopes(
        stored_events: Vec<Self>,
    ) -> Result<Vec<SuiEventEnvelope>, anyhow::Error> {
        let mut events = Vec::with_capacity(stored_events.len());
        for stored_event in stored_events {
            let event_envelope: SuiEventEnvelope = stored_event.try_into()?;
            events.push(event_envelope);
        }
        Ok(events)
    }

    fn package_id(&self) -> Result<ObjectID, anyhow::Error> {
        self.package_id.ok_or_else(|| {
            let msg = format!("Missing package_id for event {:?}", self);
            // error!(msg);
            anyhow::anyhow!(StorageMissingFieldError(msg))
        })
    }

    fn transaction_module(&self) -> Result<String, anyhow::Error> {
        let module_name = self.module_name.as_ref().ok_or_else(|| {
            let msg = format!("Missing transaction_module for event {:?}", self);
            // FIXME log in the upper layer
            // error!(msg);
            anyhow::anyhow!(StorageMissingFieldError(msg))
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

    fn object_id(&self) -> Result<ObjectID, anyhow::Error> {
        self.object_id.ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing object_id for event {:?}",
                self
            )))
        })
    }

    fn object_version(&self) -> Result<SequenceNumber, anyhow::Error> {
        self.object_version.ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing object_version for event {:?}",
                self
            )))
        })
    }

    fn transfer_type(&self) -> Result<&TransferType, anyhow::Error> {
        self.transfer_type.as_ref().ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing transfer_type for event {:?}",
                self
            )))
        })
    }
}

impl TryInto<SuiEventEnvelope> for StoredEvent {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<SuiEventEnvelope, Self::Error> {
        let timestamp = self.timestamp;
        let tx_digest = self.tx_digest;
        let event_type_str = self.event_type.as_str();
        let event = match EventType::from_str(event_type_str) {
            Ok(type_) => {
                match type_ {
                    EventType::MoveEvent => self.into_move_event(),
                    EventType::Publish => self.into_publish(),
                    EventType::TransferObject => self.into_transfer_object(),
                    EventType::DeleteObject => self.into_delete_object(),
                    EventType::NewObject => self.into_new_object(),
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
    /// Adds events to the EventStore.
    /// Semantics: events are appended.  The sequence number must be nondecreasing - EventEnvelopes
    /// which have sequence numbers below the current one will be skipped.  This feature
    /// is intended for deduplication.
    /// Returns Ok(rows_affected).
    async fn add_events(
        &self,
        events: &[EventEnvelope],
        checkpoint_num: u64,
    ) -> Result<u64, SuiError>;

    /// Queries for events emitted by a given transaction, returned in order emitted
    /// NOTE: Not all events come from transactions
    async fn events_for_transaction(
        &self,
        digest: TransactionDigest,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Queries for all events of a certain EventType within a given time window.
    /// Will return at most limit of the most recent events within the window, sorted in descending time.
    async fn events_by_type(
        &self,
        start_time: u64,
        end_time: u64,
        event_type: EventType,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Generic event iteration bounded by time.  Return in ingestion order.
    /// start_time is inclusive and end_time is exclusive.
    async fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Generic event iteration bounded by checkpoint number.  Return in ingestion order.
    /// Checkpoint numbers are inclusive on both ends.
    fn events_by_checkpoint(
        &self,
        start_checkpoint: u64,
        end_checkpoint: u64,
    ) -> Result<StreamedResult, SuiError>;

    /// Queries all Move events belonging to a certain Module ID within a given time window.
    /// Will return at most limit of the most recent events within the window, sorted in descending time.
    async fn events_by_module_id(
        &self,
        start_time: u64,
        end_time: u64,
        module: ModuleId,
        limit: usize,
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
