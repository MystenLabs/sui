// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::ensure;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::IdentStr;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::StructTag;
use move_core_types::value::MoveStruct;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;
use serde_with::Bytes;
use strum::IntoStaticStr;
use strum::VariantNames;
use strum_macros::{EnumDiscriminants, EnumVariantNames};
use tracing::error;

use crate::error::SuiError;
use crate::object::MoveObject;
use crate::object::ObjectFormatOptions;
use crate::object::Owner;
use crate::storage::SingleTxContext;
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
    /// Transaction sequence number, must be nondecreasing for event ingestion idempotency
    pub seq_num: u64,
    /// Consecutive per-tx counter assigned to this event.
    pub event_num: u64,
    /// Specific event type
    pub event: Event,
    /// json value for MoveStruct (for MoveEvent only)
    pub move_struct_json_value: Option<Value>,
}
/// Unique ID of a Sui Event, the ID is a combination of tx seq number and event seq number,
/// the ID is local to this particular fullnode and will be different from other fullnode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventID {
    pub tx_seq: i64,
    pub event_seq: i64,
}

impl From<(i64, i64)> for EventID {
    fn from((tx_seq_num, event_seq_number): (i64, i64)) -> Self {
        Self {
            tx_seq: tx_seq_num as i64,
            event_seq: event_seq_number as i64,
        }
    }
}

impl From<EventID> for String {
    fn from(id: EventID) -> Self {
        format!("{}:{}", id.tx_seq, id.event_seq)
    }
}

impl TryFrom<String> for EventID {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let values = value.split(':').collect::<Vec<_>>();
        ensure!(values.len() == 2, "Malformed EventID : {value}");
        Ok((i64::from_str(values[0])?, i64::from_str(values[1])?).into())
    }
}

impl EventEnvelope {
    pub fn new(
        timestamp: u64,
        tx_digest: Option<TransactionDigest>,
        seq_num: u64,
        event_num: u64,
        event: Event,
        move_struct_json_value: Option<Value>,
    ) -> Self {
        Self {
            timestamp,
            tx_digest,
            seq_num,
            event_num,
            event,
            move_struct_json_value,
        }
    }

    pub fn event_type(&self) -> &'static str {
        (&self.event).into()
    }
}

#[derive(
    EnumVariantNames,
    Eq,
    Debug,
    strum_macros::Display,
    Copy,
    Clone,
    strum_macros::EnumString,
    PartialEq,
    Deserialize,
    Serialize,
    Hash,
    JsonSchema,
    EnumDiscriminants,
)]
#[strum_discriminants(name(TransferTypeVariants))]
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
    Deserialize,
    Serialize,
    Hash,
    EnumDiscriminants,
    EnumVariantNames,
    IntoStaticStr,
)]
#[strum_discriminants(derive(strum_macros::EnumString))]
#[strum_discriminants(name(EventType), derive(Serialize, Deserialize, JsonSchema))]
// Developer note: PLEASE only append new entries, do not modify existing entries (binary compact)
pub enum Event {
    /// Transaction level event
    /// Move-specific event
    MoveEvent {
        package_id: ObjectID,
        transaction_module: Identifier,
        sender: SuiAddress,
        type_: StructTag,
        #[serde_as(as = "Bytes")]
        contents: Vec<u8>,
    },
    /// Module published
    Publish {
        sender: SuiAddress,
        package_id: ObjectID,
    },
    /// Coin balance changing event
    CoinBalanceChange {
        package_id: ObjectID,
        transaction_module: Identifier,
        sender: SuiAddress,
        change_type: BalanceChangeType,
        owner: Owner,
        coin_type: String,
        coin_object_id: ObjectID,
        version: SequenceNumber,
        /// The amount indicate the coin value changes for this event,
        /// negative amount means spending coin value and positive means receiving coin value.
        amount: i128,
    },
    /// Epoch change
    EpochChange(EpochId),
    /// New checkpoint
    Checkpoint(CheckpointSequenceNumber),

    /// Object level event
    /// Transfer objects to new address / wrap in another object
    TransferObject {
        package_id: ObjectID,
        transaction_module: Identifier,
        sender: SuiAddress,
        recipient: Owner,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    },

    /// Object level event
    /// Object mutated.
    MutateObject {
        package_id: ObjectID,
        transaction_module: Identifier,
        sender: SuiAddress,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// Delete object
    DeleteObject {
        package_id: ObjectID,
        transaction_module: Identifier,
        sender: SuiAddress,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// New object creation
    NewObject {
        package_id: ObjectID,
        transaction_module: Identifier,
        sender: SuiAddress,
        recipient: Owner,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    },
}

#[derive(
    EnumVariantNames,
    Eq,
    Debug,
    strum_macros::Display,
    Copy,
    Clone,
    strum_macros::EnumString,
    PartialEq,
    Deserialize,
    Serialize,
    Hash,
    JsonSchema,
    EnumDiscriminants,
)]
#[strum_discriminants(name(BalanceChangeTypeVariants))]
pub enum BalanceChangeType {
    Gas,
    Pay,
    Receive,
}

impl Event {
    pub fn move_event(
        package_id: &AccountAddress,
        module: &IdentStr,
        sender: SuiAddress,
        type_: StructTag,
        contents: Vec<u8>,
    ) -> Self {
        Event::MoveEvent {
            package_id: ObjectID::from(*package_id),
            transaction_module: Identifier::from(module),
            sender,
            type_,
            contents,
        }
    }

    pub fn balance_change(
        ctx: &SingleTxContext,
        change_type: BalanceChangeType,
        owner: Owner,
        coin_object_id: ObjectID,
        version: SequenceNumber,
        object_type: &StructTag,
        amount: i128,
    ) -> Self {
        // We know this is a Coin object, safe to unwrap.
        let coin_type = object_type.type_params[0].to_string();
        Event::CoinBalanceChange {
            package_id: ctx.package_id,
            transaction_module: ctx.transaction_module.clone(),
            sender: ctx.sender,
            change_type,
            owner,
            coin_type,
            coin_object_id,
            version,
            amount,
        }
    }

    pub fn transfer_object(
        ctx: &SingleTxContext,
        recipient: Owner,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Self {
        Self::TransferObject {
            package_id: ctx.package_id,
            transaction_module: ctx.transaction_module.clone(),
            sender: ctx.sender,
            recipient,
            object_type,
            object_id,
            version,
        }
    }

    pub fn new_object(
        ctx: &SingleTxContext,
        recipient: Owner,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Self {
        Event::NewObject {
            package_id: ctx.package_id,
            transaction_module: ctx.transaction_module.clone(),
            sender: ctx.sender,
            recipient,
            object_type,
            object_id,
            version,
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
            Event::TransferObject { object_id, .. }
            | Event::MutateObject { object_id, .. }
            | Event::DeleteObject { object_id, .. }
            | Event::NewObject { object_id, .. }
            | Event::CoinBalanceChange {
                coin_object_id: object_id,
                ..
            } => Some(*object_id),
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
            | Event::MutateObject { package_id, .. }
            | Event::CoinBalanceChange { package_id, .. }
            | Event::Publish { package_id, .. } => Some(*package_id),
            _ => None,
        }
    }

    /// Extracts the Sender address associated with the event.
    pub fn sender(&self) -> Option<SuiAddress> {
        match self {
            Event::MoveEvent { sender, .. }
            | Event::TransferObject { sender, .. }
            | Event::MutateObject { sender, .. }
            | Event::NewObject { sender, .. }
            | Event::Publish { sender, .. }
            | Event::DeleteObject { sender, .. }
            | Event::CoinBalanceChange { sender, .. } => Some(*sender),
            _ => None,
        }
    }

    /// Extract a module name, if available, from a SuiEvent
    // TODO: should we switch to IdentStr or &str?  These are more complicated to make work due to lifetimes
    pub fn module_name(&self) -> Option<&str> {
        match self {
            Event::MoveEvent {
                transaction_module, ..
            }
            | Event::NewObject {
                transaction_module, ..
            }
            | Event::DeleteObject {
                transaction_module, ..
            }
            | Event::TransferObject {
                transaction_module, ..
            }
            | Event::MutateObject {
                transaction_module, ..
            }
            | Event::CoinBalanceChange {
                transaction_module, ..
            } => Some(transaction_module.as_str()),
            _ => None,
        }
    }

    /// Extracts the recipient from a SuiEvent, if available
    pub fn recipient(&self) -> Option<&Owner> {
        match self {
            Event::TransferObject { recipient, .. }
            | Event::NewObject { recipient, .. }
            | Event::CoinBalanceChange {
                owner: recipient, ..
            } => Some(recipient),
            _ => None,
        }
    }

    /// Extracts the serialized recipient from a SuiEvent, if available
    pub fn recipient_serialized(&self) -> Result<Option<String>, SuiError> {
        match self.recipient() {
            Some(recipient) => {
                let res = serde_json::to_string(recipient);
                if let Err(e) = res {
                    error!("Failed to serialize recipient field of event: {:?}", self);
                    Err(SuiError::OwnerFailedToSerialize {
                        error: (e.to_string()),
                    })
                } else {
                    Ok(res.ok())
                }
            }
            None => Ok(None),
        }
    }

    /// Extracts the bcs move content from a SuiEvent, if available
    pub fn move_event_contents(&self) -> Option<&[u8]> {
        if let Event::MoveEvent { contents, .. } = self {
            Some(contents)
        } else {
            None
        }
    }

    /// Extracts the move event name (StructTag) from a SuiEvent, if available
    /// "0x2::devnet_nft::MintNFTEvent"
    pub fn move_event_name(&self) -> Option<String> {
        if let Event::MoveEvent { type_, .. } = self {
            Some(type_.to_string())
        } else {
            None
        }
    }

    /// Extracts the TransferType from a SuiEvent, if available
    pub fn object_type(&self) -> Option<String> {
        match self {
            Event::TransferObject { object_type, .. }
            | Event::MutateObject { object_type, .. }
            | Event::NewObject { object_type, .. }
            | Event::CoinBalanceChange {
                coin_type: object_type,
                ..
            } => Some(object_type.clone()),
            _ => None,
        }
    }

    /// Extracts the Object Version from a SuiEvent, if available
    pub fn object_version(&self) -> Option<&SequenceNumber> {
        match self {
            Event::TransferObject { version, .. }
            | Event::MutateObject { version, .. }
            | Event::CoinBalanceChange { version, .. }
            | Event::NewObject { version, .. }
            | Event::DeleteObject { version, .. } => Some(version),
            _ => None,
        }
    }

    /// Extracts the amount from a SuiEvent::CoinBalanceChange event
    pub fn amount(&self) -> Option<i128> {
        if let Event::CoinBalanceChange { amount, .. } = self {
            Some(*amount)
        } else {
            None
        }
    }

    /// Extracts the balance change type from a SuiEvent::CoinBalanceChange event
    pub fn balance_change_type(&self) -> Option<&BalanceChangeType> {
        if let Event::CoinBalanceChange { change_type, .. } = self {
            Some(change_type)
        } else {
            None
        }
    }

    pub fn balance_change_from_ordinal(ordinal: usize) -> Result<BalanceChangeType, SuiError> {
        BalanceChangeType::from_str(BalanceChangeType::VARIANTS[ordinal]).map_err(|e| {
            SuiError::BadObjectType {
                error: format!(
                    "Could not parse balance change type from ordinal: {ordinal} into BalanceChangeType: {e:?}"
                ),
            }
        })
    }

    pub fn move_event_to_move_struct(
        type_: &StructTag,
        contents: &[u8],
        resolver: &impl GetModule,
    ) -> Result<MoveStruct, SuiError> {
        let layout = MoveObject::get_layout_from_struct_tag(
            type_.clone(),
            ObjectFormatOptions::default(),
            resolver,
        )?;
        MoveStruct::simple_deserialize(contents, &layout).map_err(|e| {
            SuiError::ObjectSerializationError {
                error: e.to_string(),
            }
        })
    }
}
