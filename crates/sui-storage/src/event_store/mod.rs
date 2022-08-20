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
use std::collections::BTreeMap;
use std::str::FromStr;
use sui_json_rpc_types::{SuiEvent, SuiEventEnvelope};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest};
use sui_types::error::SuiError;
use sui_types::error::SuiError::{StorageCorruptedFieldError, StorageMissingFieldError};
use sui_types::event::{Event, TransferType};
use sui_types::event::{EventEnvelope, EventType};
use sui_types::object::Owner;
use tokio_stream::StreamExt;

pub mod sql;
pub mod test_utils;
pub use sql::SqlEventStore;

use flexstr::SharedStr;

/// Maximum number of events one can ask for right now
pub const EVENT_STORE_QUERY_MAX_LIMIT: usize = 100;

pub const TRANSFER_TYPE_KEY: &str = "xfer_type";
pub const OBJECT_VERSION_KEY: &str = "obj_ver";

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
        let version = self.object_version()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract object version from StoredEvent: {self:?}")
        })?;
        let type_ = self.transfer_type()?.ok_or_else(|| {
            anyhow::anyhow!("Can't extract transfer type from StoredEvent: {self:?}")
        })?;
        Ok(SuiEvent::TransferObject {
            package_id,
            transaction_module,
            sender,
            recipient,
            object_id,
            version,
            type_,
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

    fn object_id(&self) -> Result<ObjectID, anyhow::Error> {
        self.object_id.ok_or_else(|| {
            anyhow::anyhow!(StorageMissingFieldError(format!(
                "Missing object_id for event {:?}",
                self
            )))
        })
    }

    fn object_version(&self) -> Result<Option<SequenceNumber>, anyhow::Error> {
        let object_version = self.fields.get(OBJECT_VERSION_KEY);
        match object_version {
            Some(EventValue::Json(serde_json::Value::Number(object_version_ordinal))) => {
                let object_version_ordinal = object_version_ordinal.as_u64().ok_or_else(|| {
                    SuiError::ExtraFieldFailedToDeserialize {
                        error: format!(
                            "Error parsing object version from extra fields: {object_version:?}"
                        ),
                    }
                })?;
                Ok(Some(SequenceNumber::from_u64(object_version_ordinal)))
            }
            None => Ok(None),
            Some(other_value) => anyhow::bail!(SuiError::ExtraFieldFailedToDeserialize {
                error: format!("Got unexpected stored value for object_version: {other_value:?}"),
            }),
        }
    }

    fn transfer_type(&self) -> Result<Option<TransferType>, anyhow::Error> {
        let transfer_type = self.fields.get(TRANSFER_TYPE_KEY);
        match transfer_type {
            Some(EventValue::Json(serde_json::Value::Number(transfer_type_ordinal))) => {
                let transfer_type_ordinal = transfer_type_ordinal.as_u64().ok_or_else(|| {
                    SuiError::ExtraFieldFailedToDeserialize {
                        error: format!(
                            "Error parsing transfer type from extra fields: {transfer_type:?}"
                        ),
                    }
                })?;
                Ok(Some(Event::transfer_type_from_ordinal(
                    transfer_type_ordinal as usize,
                )?))
            }
            None => Ok(None),
            Some(other_value) => anyhow::bail!(SuiError::ExtraFieldFailedToDeserialize {
                error: format!("Got unexpected stored value for transfer_type: {other_value:?}"),
            }),
        }
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

    /// Returns at most `limit` events emitted by a given
    /// transaction, sorted in order emitted.
    async fn events_by_transaction(
        &self,
        digest: TransactionDigest,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events of a certain EventType
    /// (e.g. `TransferObject`) within [start_time, end_time),
    /// sorted in in descending time.
    async fn events_by_type(
        &self,
        start_time: u64,
        end_time: u64,
        event_type: EventType,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events emitted in a certain Module ID during
    /// [start_time, end_time), sorted in descending time.
    async fn events_by_module_id(
        &self,
        start_time: u64,
        end_time: u64,
        module: &ModuleId,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events with the move event struct name
    /// (e.g. `0x2::devnet_nft::MintNFTEvent`) emitted
    /// during [start_time, end_time), sorted in descending time.
    async fn events_by_move_event_struct_name(
        &self,
        start_time: u64,
        end_time: u64,
        move_event_struct_name: &str,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events associated with a certain sender
    /// emitted during [start_time, end_time), sorted in descending time.
    async fn events_by_sender(
        &self,
        start_time: u64,
        end_time: u64,
        sender: &SuiAddress,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events associated with a certain recipient
    /// emitted during [start_time, end_time), sorted in descending time.
    async fn events_by_recipient(
        &self,
        start_time: u64,
        end_time: u64,
        recipient: &Owner,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Returns at most `limit` events associated with a certain object id
    /// emitted during [start_time, end_time), sorted in descending time.
    async fn events_by_object(
        &self,
        start_time: u64,
        end_time: u64,
        object: &ObjectID,
        limit: usize,
    ) -> Result<Vec<StoredEvent>, SuiError>;

    /// Generic event iterator to return events emitted in checkpoints
    /// [start_checkpoint, end_checkpoint), order not guaranteed.
    fn events_by_checkpoint(
        &self,
        start_checkpoint: u64,
        end_checkpoint: u64,
    ) -> Result<StreamedResult, SuiError>;

    /// Generic event iterator that returns events emitted between
    /// [start_time, end_time), sorted in descending time.
    async fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
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
