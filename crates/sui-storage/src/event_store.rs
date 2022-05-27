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

use move_core_types::language_storage::ModuleId;
use move_core_types::value::MoveValue;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::event::{EventEnvelope, EventType};

use flexstr::SharedStr;

/// One event pulled out from the EventStore
#[allow(unused)]
pub struct StoredEvent {
    /// UTC timestamp in milliseconds
    timestamp: u64,
    checkpoint_num: u64,
    /// Not present for non-transaction System events (eg EpochChange)
    tx_digest: Option<TransactionDigest>,
    /// The variant name from SuiEvent, eg MoveEvent, Publish, etc.
    event_type: SharedStr,
    /// Object ID of the Move package generating the event
    package_id: Option<ObjectID>,
    /// Module name of the Move package generating the event
    module_name: Option<SharedStr>,
    /// Individual event fields.  As much as possible these should be deconstructed and flattened,
    /// ie `{'obj': {'fieldA': 'A', 'fieldB': 'B'}}` should really be broken down to
    /// `[('obj.fieldA', 'A'), ('obj.fieldB', 'B')]
    ///
    /// ## Common field names
    /// * `object_id` - used by TransferObject, DeleteObject
    /// * `version` - used by TransferObject
    fields: Vec<(SharedStr, EventValue)>, // Change this to something based on CBOR for binary values, or our own value types for efficiency
}

/// Enum for different types of values returnable from events in the EventStore
// This is distinct from MoveValue because we want to explicitly represent (and translate)
// blobs and strings, allowing us to use more efficient representations.
pub enum EventValue {
    Move(MoveValue),
    /// Efficient string representation, no allocation for small strings
    String(SharedStr),
    /// Arbitrary-length blob.  Please use MoveValue::Address for ObjectIDs and similar things.
    BinaryBlob(Vec<u8>),
}

/// An EventStore supports event ingestion and flexible event querying
/// One can think of events as logs.  They represent a log of what is happening to Sui.
/// Thus, all different kinds of events fit on a timeline, and one should be able to query for
/// different types of events that happen over that timeline.
trait EventStore<EventIt>
where
    EventIt: Iterator<Item = StoredEvent>,
{
    /// Adds events to the EventStore.
    /// Semantics: events are appended, no deduplication is done.
    fn add_events(
        &self,
        events: &[EventEnvelope],
        checkpoint_num: u64,
    ) -> Result<(), EventStoreError>;

    /// Queries for events emitted by a given transaction, returned in order emitted
    /// NOTE: Not all events come from transactions
    fn events_for_transaction(&self, digest: TransactionDigest)
        -> Result<EventIt, EventStoreError>;

    /// Queries for all events of a certain EventType within a given time window.
    /// Will return at most limit of the most recent events within the window, sorted in ascending time.
    fn events_by_type(
        &self,
        start_time: u64,
        end_time: u64,
        event_type: EventType,
        limit: usize,
    ) -> Result<EventIt, EventStoreError>;

    /// Generic event iteration bounded by time.  Return in ingestion order.
    fn event_iterator(&self, start_time: u64, end_time: u64) -> Result<EventIt, EventStoreError>;

    /// Generic event iteration bounded by checkpoint number.  Return in ingestion order.
    /// Checkpoint numbers are inclusive on both ends.
    fn events_by_checkpoint(
        &self,
        start_checkpoint: u64,
        end_checkpoint: u64,
    ) -> Result<EventIt, EventStoreError>;

    /// Queries all Move events belonging to a certain Module ID within a given time window.
    /// Will return at most limit of the most recent events within the window, sorted in ascending time.
    fn events_by_module_id(
        &self,
        start_time: u64,
        end_time: u64,
        module: ModuleId,
        limit: usize,
    ) -> Result<EventIt, EventStoreError>;
}

pub enum EventStoreError {
    GenericError(Box<dyn std::error::Error>),
}
