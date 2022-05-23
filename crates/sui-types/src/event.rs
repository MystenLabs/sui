// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use name_variant::NamedVariant;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};

use crate::{base_types::{SuiAddress, ObjectID, SequenceNumber, TransactionDigest}, committee::EpochId, messages::Transaction};

/// User-defined event emitted by executing Move code.
/// Executing a transaction produces an ordered log of these
#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct Event {
    pub type_: StructTag,
    #[serde_as(as = "Bytes")]
    pub contents: Vec<u8>,
}

impl Event {
    pub fn new(type_: StructTag, contents: Vec<u8>) -> Self {
        Event { type_, contents }
    }
}


/// A universal Sui event type encapsulating different types of events
#[derive(Debug, Clone, PartialEq)]
pub struct EventEnvelope {
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    timestamp: u64,
    /// Transaction digest of associated transaction, if any
    tx_digest: Option<TransactionDigest>,
    /// Specific event type
    event: SuiEvent,
}

impl EventEnvelope {
    pub fn new(timestamp: u64, tx_digest: Option<TransactionDigest>, event: SuiEvent) -> Self {
        Self {
            timestamp, tx_digest, event
        }
    }

    pub fn event_type(&self) -> &'static str {
        self.event.variant_name()
    }
}

/// Specific type of event
#[derive(Debug, Clone, PartialEq, NamedVariant)]
pub enum SuiEvent {
    /// Move-specific event
    MoveEvent(Event),
    /// Module published
    Publish { package_name: String, package_object_id: SuiAddress },
    /// Transfer
    Transfer { object_id: ObjectID, version: SequenceNumber, from: SuiAddress, to: SuiAddress },
    /// Epooch change
    EpochChange(EpochId),
}