// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use name_variant::NamedVariant;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::Bytes;
use strum::VariantNames;
use strum_macros::{EnumDiscriminants, EnumVariantNames};

use crate::{
    base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest},
    committee::EpochId,
    messages_checkpoint::CheckpointSequenceNumber,
};

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
#[strum_discriminants(name(EventType), derive(Serialize, Deserialize, JsonSchema))]
// Developer note: PLEASE only append new entries, do not modify existing entries (binary compat)
pub enum Event {
    /// Move-specific event
    MoveEvent(MoveEventObject),
    /// Module published
    Publish {
        sender: SuiAddress,
        transaction_digest: TransactionDigest,
        package_id: ObjectID,
    },
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

#[serde_as]
#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub struct MoveEventObject {
    pub package: ObjectID,
    pub module: Identifier,
    pub function: Identifier,
    pub sender: SuiAddress,
    pub transaction_digest: TransactionDigest,
    pub type_: StructTag,
    #[serde_as(as = "Bytes")]
    pub contents: Vec<u8>,
}

impl Event {
    pub fn move_event(
        package: ObjectID,
        module: Identifier,
        function: Identifier,
        transaction_digest: TransactionDigest,
        sender: SuiAddress,
        type_: StructTag,
        contents: Vec<u8>,
    ) -> Self {
        Event::MoveEvent(MoveEventObject {
            package,
            module,
            function,
            sender,
            transaction_digest,
            type_,
            contents,
        })
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
            Event::MoveEvent(event_obj) => Some(event_obj.package),
            Event::Publish { package_id, .. } => Some(*package_id),
            _ => None,
        }
    }

    /// Extracts the Sender address associated with the event.
    pub fn sender(&self) -> Option<SuiAddress> {
        match self {
            Event::MoveEvent(event_obj) => Some(event_obj.sender),
            Event::Publish { sender, .. } => Some(*sender),
            _ => None,
        }
    }

    /// Extract a module name, if available, from a SuiEvent
    // TODO: should we switch to IdentStr or &str?  These are more complicated to make work due to lifetimes
    pub fn module_name(&self) -> Option<&str> {
        match self {
            Event::MoveEvent(event_obj) => Some(event_obj.module.as_ident_str().as_str()),
            _ => None,
        }
    }

    /// Extracts the function name from a SuiEvent, if available
    pub fn function_name(&self) -> Option<&str> {
        match self {
            Event::MoveEvent(event_obj) => Some(event_obj.function.as_ident_str().as_str()),
            _ => None,
        }
    }
}
