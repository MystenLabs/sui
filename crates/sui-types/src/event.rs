// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::{ModuleId, StructTag};
use name_variant::NamedVariant;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};

use crate::{
    base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest},
    committee::EpochId,
    messages_checkpoint::CheckpointSequenceNumber,
};

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
            timestamp,
            tx_digest,
            event,
        }
    }

    pub fn event_type(&self) -> &'static str {
        self.event.variant_name()
    }
}

/// Specific type of event
#[serde_as]
#[derive(Eq, Debug, Clone, PartialEq, NamedVariant, Deserialize, Serialize, Hash)]
pub enum SuiEvent {
    /// Move-specific event
    MoveEvent {
        type_: StructTag,
        #[serde_as(as = "Bytes")]
        contents: Vec<u8>,
    },
    /// Module published
    Publish { package_id: ObjectID },
    /// Transfer coin
    TransferCoin {
        object_id: ObjectID,
        version: SequenceNumber,
        destination_addr: SuiAddress,
    },
    /// Epooch change
    EpochChange(EpochId),
    /// New checkpoint
    Checkpoint(CheckpointSequenceNumber),
}

impl SuiEvent {
    pub fn move_event(type_: StructTag, contents: Vec<u8>) -> Self {
        SuiEvent::MoveEvent { type_, contents }
    }

    /// Extract a module ID, if available, from a SuiEvent
    pub fn module_id(&self) -> Option<ModuleId> {
        match self {
            SuiEvent::MoveEvent {
                type_: struct_tag, ..
            } => Some(struct_tag.module_id()),
            _ => None,
        }
    }
}
