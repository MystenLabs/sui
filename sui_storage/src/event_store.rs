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

use sui_types::base_types::TransactionDigest;
use sui_types::event::EventEnvelope;

use flexstr::SharedStr;
use serde_json::Value;


/// One event pulled out from the EventStore
struct StoredEvent {
  /// UTC timestamp in milliseconds
  timestamp: u64,
  checkpoint_num: u64,
  /// Only present for events pertaining to specific transactions
  tx_digest: Option<TransactionDigest>,
  /// The variant name from SuiEvent, eg MoveEvent, Publish, etc.
  event_type: SharedStr,
  /// Will be None for System events
  move_module: Option<SharedStr>,
  /// Individual event fields.  As much as possible these should be deconstructed and flattened,
  /// ie `{'obj': {'fieldA': 'A', 'fieldB': 'B'}}` should really be broken down to
  // `[('obj.fieldA', 'A'), ('obj.fieldB', 'B')]
  fields: Vec<(SharedStr, Value)>,
}

/// An EventStore supports event ingestion and flexible event querying
trait EventStore {
  /// Adds events to the EventStore
  fn add_events(&self, events: &[EventEnvelope]) -> Result<(), StorageError>;

  /// Queries for events emitted by a given transaction, returned in order emitted
  /// NOTE: Not all events come from transactions
  fn events_for_transaction(&self, digest: TransactionDigest) -> Result<Vec<StoredEvent>, StorageError>;
}

pub enum StorageError {
  GenericError(Box<dyn std::error::Error>),
}