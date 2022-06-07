// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use name_variant::NamedVariant;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::VariantNames;
use strum_macros::{EnumDiscriminants, EnumVariantNames};

use crate::object::MoveObject;
use crate::{
    base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest},
    committee::EpochId,
    messages_checkpoint::CheckpointSequenceNumber,
};
use schemars::JsonSchema;
use serde_with::serde_as;

/// A universal Sui event type encapsulating different types of events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    pub timestamp: u64,
    /// Transaction digest of associated transaction, if any
    pub tx_digest: Option<TransactionDigest>,
    /// Specific event type
    pub event: Event,
    /// json value for MoveStruct (for MoveEvent only)
    pub move_struct_json_value: Option<Value>,
}

impl EventEnvelope {
    pub fn new(
        timestamp: u64,
        tx_digest: Option<TransactionDigest>,
        event: Event,
        move_struct_json_value: Option<Value>,
    ) -> Self {
        Self {
            timestamp,
            tx_digest,
            event,
            move_struct_json_value,
        }
    }

    pub fn event_type(&self) -> &'static str {
        self.event.variant_name()
    }
}

#[derive(
    Eq, Debug, strum_macros::Display, Clone, PartialEq, Deserialize, Serialize, Hash, JsonSchema,
)]
pub enum TransferType {
    Coin,
    ToAddress,
    ToObject, // wrap object in another object
}

/// Specific type of event
#[serde_as]
#[derive(
    Eq,
    Debug,
    Clone,
    PartialEq,
    NamedVariant,
    Deserialize,
    Serialize,
    Hash,
    EnumDiscriminants,
    EnumVariantNames,
)]
#[strum_discriminants(name(EventType))]
// Developer note: PLEASE only append new entries, do not modify existing entries (binary compat)
pub enum Event {
    /// Move-specific event
    MoveEvent(MoveObject),
    /// Module published
    Publish { package_id: ObjectID },
    /// Transfer objects to new address / wrap in another object / coin
    TransferObject {
        object_id: ObjectID,
        version: SequenceNumber,
        destination_addr: SuiAddress,
        type_: TransferType,
    },
    /// Delete object
    DeleteObject(ObjectID),
    /// New object creation
    NewObject(ObjectID),
    /// Epoch change
    EpochChange(EpochId),
    /// New checkpoint
    Checkpoint(CheckpointSequenceNumber),
}

impl Event {
    pub fn move_event(type_: StructTag, contents: Vec<u8>) -> Self {
        Event::MoveEvent(MoveObject::new(type_, contents))
    }

    pub fn name_from_ordinal(ordinal: usize) -> &'static str {
        Event::VARIANTS[ordinal]
    }

    /// Returns the EventType associated with an Event
    pub fn event_type(&self) -> EventType {
        self.into()
    }

    /// Returns the object or package ID associated with the event, if available.  Specifically:
    /// - For TransferObject: the object ID being transferred (eg moving child from parent, its the child)
    /// - for DeleteObject and NewObject, the Object ID
    pub fn object_id(&self) -> Option<ObjectID> {
        match self {
            Event::TransferObject { object_id, .. } => Some(*object_id),
            Event::DeleteObject(obj_id) => Some(*obj_id),
            Event::NewObject(obj_id) => Some(*obj_id),
            _ => None,
        }
    }

    /// Extracts the Move package ID associated with the event, or the package published.
    pub fn package_id(&self) -> Option<ObjectID> {
        match self {
            Event::MoveEvent(event_obj) => Some(event_obj.type_.address.into()),
            Event::Publish { package_id } => Some(*package_id),
            _ => None,
        }
    }

    /// Extract a module name, if available, from a SuiEvent
    // TODO: should we switch to IdentStr or &str?  These are more complicated to make work due to lifetimes
    pub fn module_name(&self) -> Option<&str> {
        match self {
            Event::MoveEvent(event) => Some(event.type_.module.as_ident_str().as_str()),
            _ => None,
        }
    }

    /// Extracts the function name from a SuiEvent, if available
    pub fn function_name(&self) -> Option<String> {
        match self {
            Event::MoveEvent(event_obj) => Some(event_obj.type_.name.to_string()),
            _ => None,
        }
    }
}
