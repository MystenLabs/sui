// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_bytecode_utils::{layout::TypeLayoutBuilder, module_cache::GetModule};
use move_core_types::{
    language_storage::{StructTag, TypeTag},
    value::{MoveStruct, MoveTypeLayout},
};
use name_variant::NamedVariant;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{serde_as, Bytes};
use strum::VariantNames;
use strum_macros::{EnumDiscriminants, EnumVariantNames};

use crate::{
    base_types::{ObjectID, SequenceNumber, SuiAddress, TransactionDigest},
    committee::EpochId,
    error::SuiError,
    messages_checkpoint::CheckpointSequenceNumber,
};

/// A universal Sui event type encapsulating different types of events
#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Eq, Debug, strum_macros::Display, Clone, PartialEq, Deserialize, Serialize, Hash)]
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
    MoveEvent {
        type_: StructTag,
        #[serde_as(as = "Bytes")]
        contents: Vec<u8>,
    },
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
    /// Epooch change
    EpochChange(EpochId),
    /// New checkpoint
    Checkpoint(CheckpointSequenceNumber),
}

impl Event {
    pub fn move_event(type_: StructTag, contents: Vec<u8>) -> Self {
        Event::MoveEvent { type_, contents }
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
            Event::MoveEvent { type_, .. } => Some(type_.address.into()),
            Event::Publish { package_id } => Some(*package_id),
            _ => None,
        }
    }

    /// Extract a module name, if available, from a SuiEvent
    // TODO: should we switch to IdentStr or &str?  These are more complicated to make work due to lifetimes
    pub fn module_name(&self) -> Option<&str> {
        match self {
            Event::MoveEvent {
                type_: struct_tag, ..
            } => Some(struct_tag.module.as_ident_str().as_str()),
            _ => None,
        }
    }

    /// Extracts the function name from a SuiEvent, if available
    pub fn function_name(&self) -> Option<String> {
        match self {
            Event::MoveEvent {
                type_: struct_tag, ..
            } => Some(struct_tag.name.to_string()),
            _ => None,
        }
    }

    /// Extracts a MoveStruct, if possible, from the event
    pub fn extract_move_struct(
        &self,
        resolver: &impl GetModule,
    ) -> Result<Option<MoveStruct>, SuiError> {
        match self {
            Event::MoveEvent { type_, contents } => {
                let typestruct = TypeTag::Struct(type_.clone());
                let layout =
                    TypeLayoutBuilder::build_with_fields(&typestruct, resolver).map_err(|e| {
                        SuiError::ObjectSerializationError {
                            error: e.to_string(),
                        }
                    })?;
                match layout {
                    MoveTypeLayout::Struct(l) => {
                        let s = MoveStruct::simple_deserialize(contents, &l).map_err(|e| {
                            SuiError::ObjectSerializationError {
                                error: e.to_string(),
                            }
                        })?;
                        Ok(Some(s))
                    }
                    _ => unreachable!(
                        "We called build_with_types on Struct type, should get a struct layout"
                    ),
                }
            }
            _ => Ok(None),
        }
    }
}
