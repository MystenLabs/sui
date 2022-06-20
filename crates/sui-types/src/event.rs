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

use crate::object::Owner;
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
    MoveEvent {
        package_id: ObjectID,
        module: Identifier,
        function: Identifier,
        instigator: SuiAddress,
        type_: StructTag,
        #[serde_as(as = "Bytes")]
        contents: Vec<u8>,
    },
    /// Module published
    Publish {
        instigator: SuiAddress,
        package_id: ObjectID,
    },
    /// Transfer objects to new address / wrap in another object / coin
    TransferObject {
        package_id: ObjectID,
        module: Identifier,
        function: Identifier,
        instigator: SuiAddress,
        recipient: Owner,
        object_id: ObjectID,
        version: SequenceNumber,
        destination_addr: SuiAddress,
        type_: TransferType,
    },
    /// Delete object
    DeleteObject {
        package_id: ObjectID,
        module: Identifier,
        function: Identifier,
        instigator: SuiAddress,
        object_id: ObjectID,
    },
    /// New object creation
    NewObject {
        package_id: ObjectID,
        module: Identifier,
        function: Identifier,
        instigator: SuiAddress,
        recipient: Owner,
        object_id: ObjectID,
    },
    /// Epoch change
    EpochChange(EpochId),
    /// New checkpoint
    Checkpoint(CheckpointSequenceNumber),
}

impl Event {
    pub fn move_event(
        package_id: ObjectID,
        module: Identifier,
        function: Identifier,
        sender: SuiAddress,
        type_: StructTag,
        contents: Vec<u8>,
    ) -> Self {
        Event::MoveEvent {
            package_id,
            module,
            function,
            instigator: sender,
            type_,
            contents,
        }
    }

    pub fn delete_object(
        package_id: ObjectID,
        module: Identifier,
        function: Identifier,
        sender: SuiAddress,
        object_id: ObjectID,
    ) -> Self {
        Event::DeleteObject {
            package_id,
            module,
            function,
            instigator: sender,
            object_id,
        }
    }

    pub fn new_object(
        package_id: ObjectID,
        module: Identifier,
        function: Identifier,
        sender: SuiAddress,
        recipient: Owner,
        object_id: ObjectID,
    ) -> Self {
        Event::NewObject {
            package_id,
            module,
            function,
            instigator: sender,
            recipient,
            object_id,
        }
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
            Event::DeleteObject { object_id, .. } => Some(*object_id),
            Event::NewObject { object_id, .. } => Some(*object_id),
            _ => None,
        }
    }

    /// Extracts the Move package ID associated with the event, or the package published.
    pub fn package_id(&self) -> Option<ObjectID> {
        match self {
            Event::MoveEvent { package_id, .. }
            | Event::NewObject { package_id, .. }
            | Event::DeleteObject { package_id, .. }
            | Event::TransferObject { package_id, .. }
            | Event::Publish { package_id, .. } => Some(*package_id),
            _ => None,
        }
    }

    /// Extracts the Sender address associated with the event.
    pub fn sender(&self) -> Option<SuiAddress> {
        match self {
            Event::MoveEvent {
                instigator: sender, ..
            }
            | Event::TransferObject {
                instigator: sender, ..
            }
            | Event::NewObject {
                instigator: sender, ..
            }
            | Event::Publish {
                instigator: sender, ..
            }
            | Event::DeleteObject {
                instigator: sender, ..
            } => Some(*sender),
            _ => None,
        }
    }

    /// Extract a module name, if available, from a SuiEvent
    // TODO: should we switch to IdentStr or &str?  These are more complicated to make work due to lifetimes
    pub fn module_name(&self) -> Option<&str> {
        match self {
            Event::MoveEvent { module, .. }
            | Event::NewObject { module, .. }
            | Event::DeleteObject { module, .. }
            | Event::TransferObject { module, .. } => Some(module.as_str()),
            _ => None,
        }
    }

    /// Extracts the function name from a SuiEvent, if available
    pub fn function_name(&self) -> Option<&str> {
        match self {
            Event::MoveEvent { function, .. }
            | Event::NewObject { function, .. }
            | Event::DeleteObject { function, .. }
            | Event::TransferObject { function, .. } => Some(function.as_str()),
            _ => None,
        }
    }

    /// Extracts the transfer type from a SuiEvent, if available
    pub fn transfer_type(&self) -> Option<&TransferType> {
        match self {
            Event::TransferObject { type_, .. } => Some(type_),
            _ => None,
        }
    }

    /// Extracts the recipient from a SuiEvent, if available
    pub fn recipient(&self) -> Option<&Owner> {
        match self {
            Event::TransferObject { recipient, .. } | Event::NewObject { recipient, .. } => {
                Some(recipient)
            }
            _ => None,
        }
    }
}
