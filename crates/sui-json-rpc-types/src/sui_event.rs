// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::{TryFrom, TryInto};
use std::str::FromStr;

use fastcrypto::encoding::Base64;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::identifier::Identifier;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::serde_as;

use sui_types::base_types::{
    EpochId, ObjectDigest, ObjectID, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::event::{BalanceChangeType, Event, EventEnvelope, EventID, EventType};
use sui_types::filter::EventFilter;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Owner;
use sui_types::parse_sui_struct_tag;

use crate::{type_and_fields_from_move_struct, Page, SuiMoveStruct};

pub type EventPage = Page<SuiEventEnvelope, EventID>;
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "EventEnvelope", rename_all = "camelCase")]
pub struct SuiEventEnvelope {
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    pub timestamp: u64,
    /// Transaction digest of associated transaction
    pub tx_digest: TransactionDigest,
    /// Sequential event ID, ie (transaction seq number, event seq number).
    /// 1) Serves as a unique event ID for each fullnode
    /// 2) Also serves to sequence events for the purposes of pagination and querying.
    ///    A higher id is an event seen later by that fullnode.
    /// This ID is the "cursor" for event querying.
    pub id: EventID,
    /// Specific event type
    pub event: SuiEvent,
}

#[serde_as]
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "Event", rename_all = "camelCase")]
pub enum SuiEvent {
    /// Move-specific event
    #[serde(rename_all = "camelCase")]
    MoveEvent {
        // TODO: What's the best way to serialize this using `AccountAddress::short_str_lossless` ??
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        type_: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fields: Option<SuiMoveStruct>,
        #[serde_as(as = "Base64")]
        #[schemars(with = "Base64")]
        bcs: Vec<u8>,
    },
    /// Module published
    #[serde(rename_all = "camelCase")]
    Publish {
        sender: SuiAddress,
        package_id: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
    },
    /// Coin balance changing event
    #[serde(rename_all = "camelCase")]
    CoinBalanceChange {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        change_type: BalanceChangeType,
        owner: Owner,
        coin_type: String,
        coin_object_id: ObjectID,
        version: SequenceNumber,
        amount: i128,
    },
    /// Epoch change
    EpochChange(EpochId),
    /// New checkpoint
    Checkpoint(CheckpointSequenceNumber),
    /// Transfer objects to new address / wrap in another object / coin
    #[serde(rename_all = "camelCase")]
    TransferObject {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        recipient: Owner,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// Object mutated.
    #[serde(rename_all = "camelCase")]
    MutateObject {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// Delete object
    #[serde(rename_all = "camelCase")]
    DeleteObject {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// New object creation
    #[serde(rename_all = "camelCase")]
    NewObject {
        package_id: ObjectID,
        transaction_module: String,
        sender: SuiAddress,
        recipient: Owner,
        object_type: String,
        object_id: ObjectID,
        version: SequenceNumber,
    },
}

impl TryFrom<SuiEvent> for Event {
    type Error = anyhow::Error;
    fn try_from(event: SuiEvent) -> Result<Self, Self::Error> {
        Ok(match event {
            SuiEvent::MoveEvent {
                package_id,
                transaction_module,
                sender,
                type_,
                fields: _,
                bcs,
            } => Event::MoveEvent {
                package_id,
                transaction_module: Identifier::from_str(&transaction_module)?,
                sender,
                type_: parse_sui_struct_tag(&type_)?,
                contents: bcs,
            },
            SuiEvent::Publish {
                sender,
                package_id,
                version,
                digest,
            } => Event::Publish {
                sender,
                package_id,
                version,
                digest,
            },
            SuiEvent::TransferObject {
                package_id,
                transaction_module,
                sender,
                recipient,
                object_type,
                object_id,
                version,
            } => Event::TransferObject {
                package_id,
                transaction_module: Identifier::from_str(&transaction_module)?,
                sender,
                recipient,
                object_type,
                object_id,
                version,
            },
            SuiEvent::DeleteObject {
                package_id,
                transaction_module,
                sender,
                object_id,
                version,
            } => Event::DeleteObject {
                package_id,
                transaction_module: Identifier::from_str(&transaction_module)?,
                sender,
                object_id,
                version,
            },
            SuiEvent::NewObject {
                package_id,
                transaction_module,
                sender,
                recipient,
                object_type,
                object_id,
                version,
            } => Event::NewObject {
                package_id,
                transaction_module: Identifier::from_str(&transaction_module)?,
                sender,
                recipient,
                object_type,
                object_id,
                version,
            },
            SuiEvent::EpochChange(id) => Event::EpochChange(id),
            SuiEvent::Checkpoint(seq) => Event::Checkpoint(seq),
            SuiEvent::CoinBalanceChange {
                package_id,
                transaction_module,
                sender,
                change_type,
                owner,
                coin_object_id: coin_id,
                version,
                coin_type,
                amount,
            } => Event::CoinBalanceChange {
                package_id,
                transaction_module: Identifier::from_str(&transaction_module)?,
                sender,
                change_type,
                owner,
                coin_type,
                coin_object_id: coin_id,
                version,
                amount,
            },
            SuiEvent::MutateObject {
                package_id,
                transaction_module,
                sender,
                object_type,
                object_id,
                version,
            } => Event::MutateObject {
                package_id,
                transaction_module: Identifier::from_str(&transaction_module)?,
                sender,
                object_type,
                object_id,
                version,
            },
        })
    }
}

impl SuiEvent {
    pub fn try_from(event: Event, resolver: &impl GetModule) -> Result<Self, anyhow::Error> {
        Ok(match event {
            Event::MoveEvent {
                package_id,
                transaction_module,
                sender,
                type_,
                contents,
            } => {
                let bcs = contents.to_vec();

                let (type_, fields) = if let Ok(move_struct) =
                    Event::move_event_to_move_struct(&type_, &contents, resolver)
                {
                    let (type_, field) = type_and_fields_from_move_struct(&type_, move_struct);
                    (type_, Some(field))
                } else {
                    (type_.to_string(), None)
                };

                SuiEvent::MoveEvent {
                    package_id,
                    transaction_module: transaction_module.to_string(),
                    sender,
                    type_,
                    fields,
                    bcs,
                }
            }
            Event::Publish {
                sender,
                package_id,
                version,
                digest,
            } => SuiEvent::Publish {
                sender,
                package_id,
                version,
                digest,
            },
            Event::TransferObject {
                package_id,
                transaction_module,
                sender,
                recipient,
                object_type,
                object_id,
                version,
            } => SuiEvent::TransferObject {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                recipient,
                object_type,
                object_id,
                version,
            },
            Event::DeleteObject {
                package_id,
                transaction_module,
                sender,
                object_id,
                version,
            } => SuiEvent::DeleteObject {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                object_id,
                version,
            },
            Event::NewObject {
                package_id,
                transaction_module,
                sender,
                recipient,
                object_type,
                object_id,
                version,
            } => SuiEvent::NewObject {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                recipient,
                object_type,
                object_id,
                version,
            },
            Event::EpochChange(id) => SuiEvent::EpochChange(id),
            Event::Checkpoint(seq) => SuiEvent::Checkpoint(seq),
            Event::CoinBalanceChange {
                package_id,
                transaction_module,
                sender,
                change_type,
                owner,
                coin_object_id: coin_id,
                version,
                coin_type,
                amount,
            } => SuiEvent::CoinBalanceChange {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                change_type,
                owner,
                coin_object_id: coin_id,
                version,
                coin_type,
                amount,
            },
            Event::MutateObject {
                package_id,
                transaction_module,
                sender,
                object_type,
                object_id,
                version,
            } => SuiEvent::MutateObject {
                package_id,
                transaction_module: transaction_module.to_string(),
                sender,
                object_type,
                object_id,
                version,
            },
        })
    }

    pub fn get_event_type(&self) -> String {
        match self {
            SuiEvent::MoveEvent { .. } => "MoveEvent".to_string(),
            SuiEvent::Publish { .. } => "Publish".to_string(),
            SuiEvent::TransferObject { .. } => "TransferObject".to_string(),
            SuiEvent::DeleteObject { .. } => "DeleteObject".to_string(),
            SuiEvent::NewObject { .. } => "NewObject".to_string(),
            SuiEvent::EpochChange(..) => "EpochChange".to_string(),
            SuiEvent::Checkpoint(..) => "CheckPoint".to_string(),
            SuiEvent::CoinBalanceChange { .. } => "CoinBalanceChange".to_string(),
            SuiEvent::MutateObject { .. } => "MutateObject".to_string(),
        }
    }
}

impl PartialEq<SuiEventEnvelope> for EventEnvelope {
    fn eq(&self, other: &SuiEventEnvelope) -> bool {
        self.timestamp == other.timestamp
            && self.tx_digest == other.tx_digest
            && self.event == other.event
    }
}

impl PartialEq<SuiEvent> for Event {
    fn eq(&self, other: &SuiEvent) -> bool {
        match self {
            Event::MoveEvent {
                package_id: self_package_id,
                transaction_module: self_transaction_module,
                sender: self_sender,
                type_: self_type,
                contents: self_contents,
            } => {
                if let SuiEvent::MoveEvent {
                    package_id,
                    transaction_module,
                    sender,
                    type_,
                    fields: _fields,
                    bcs,
                } = other
                {
                    package_id == self_package_id
                        && &self_transaction_module.to_string() == transaction_module
                        && self_sender == sender
                        && &self_type.to_string() == type_
                        && self_contents == bcs
                } else {
                    false
                }
            }
            Event::Publish {
                sender: self_sender,
                package_id: self_package_id,
                version: self_version,
                digest: self_digest,
            } => {
                if let SuiEvent::Publish {
                    package_id,
                    sender,
                    version,
                    digest,
                } = other
                {
                    package_id == self_package_id
                        && self_sender == sender
                        && self_version == version
                        && self_digest == digest
                } else {
                    false
                }
            }
            Event::TransferObject {
                package_id: self_package_id,
                transaction_module: self_transaction_module,
                sender: self_sender,
                recipient: self_recipient,
                object_type: self_object_type,
                object_id: self_object_id,
                version: self_version,
            } => {
                if let SuiEvent::TransferObject {
                    package_id,
                    transaction_module,
                    sender,
                    recipient,
                    object_type,
                    object_id,
                    version,
                } = other
                {
                    package_id == self_package_id
                        && &self_transaction_module.to_string() == transaction_module
                        && self_sender == sender
                        && self_recipient == recipient
                        && self_object_id == object_id
                        && self_version == version
                        && self_object_type == object_type
                } else {
                    false
                }
            }
            Event::DeleteObject {
                package_id: self_package_id,
                transaction_module: self_transaction_module,
                sender: self_sender,
                object_id: self_object_id,
                version: self_version,
            } => {
                if let SuiEvent::DeleteObject {
                    package_id,
                    transaction_module,
                    sender,
                    object_id,
                    version,
                } = other
                {
                    package_id == self_package_id
                        && &self_transaction_module.to_string() == transaction_module
                        && self_sender == sender
                        && self_object_id == object_id
                        && self_version == version
                } else {
                    false
                }
            }
            Event::NewObject {
                package_id: self_package_id,
                transaction_module: self_transaction_module,
                sender: self_sender,
                recipient: self_recipient,
                object_type: self_object_type,
                object_id: self_object_id,
                version: self_version,
            } => {
                if let SuiEvent::NewObject {
                    package_id,
                    transaction_module,
                    sender,
                    recipient,
                    object_type,
                    object_id,
                    version,
                } = other
                {
                    package_id == self_package_id
                        && &self_transaction_module.to_string() == transaction_module
                        && self_sender == sender
                        && self_recipient == recipient
                        && self_object_id == object_id
                        && self_object_type == object_type
                        && self_version == version
                } else {
                    false
                }
            }
            Event::EpochChange(self_id) => {
                if let SuiEvent::EpochChange(id) = other {
                    self_id == id
                } else {
                    false
                }
            }
            Event::Checkpoint(self_id) => {
                if let SuiEvent::Checkpoint(id) = other {
                    self_id == id
                } else {
                    false
                }
            }
            Event::CoinBalanceChange {
                package_id: self_package_id,
                transaction_module: self_transaction_module,
                sender: self_sender,
                change_type: self_change_type,
                owner: self_owner,
                coin_object_id: self_coin_id,
                version: self_version,
                coin_type: self_coin_type,
                amount: self_amount,
            } => {
                if let SuiEvent::CoinBalanceChange {
                    package_id,
                    transaction_module,
                    sender,
                    change_type,
                    owner,
                    coin_object_id,
                    version,
                    coin_type,
                    amount,
                } = other
                {
                    package_id == self_package_id
                        && &self_transaction_module.to_string() == transaction_module
                        && self_owner == owner
                        && self_coin_id == coin_object_id
                        && self_version == version
                        && &self_coin_type.to_string() == coin_type
                        && self_amount == amount
                        && self_sender == sender
                        && self_change_type == change_type
                } else {
                    false
                }
            }
            Event::MutateObject {
                package_id: self_package_id,
                transaction_module: self_transaction_module,
                sender: self_sender,
                object_type: self_object_type,
                object_id: self_object_id,
                version: self_version,
            } => {
                if let SuiEvent::MutateObject {
                    package_id,
                    transaction_module,
                    sender,
                    object_type,
                    object_id,
                    version,
                } = other
                {
                    package_id == self_package_id
                        && &self_transaction_module.to_string() == transaction_module
                        && self_sender == sender
                        && self_object_type == object_type
                        && self_object_id == object_id
                        && self_version == version
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, JsonSchema, Debug)]
#[serde(rename = "EventFilter")]
pub enum SuiEventFilter {
    Package(ObjectID),
    Module(String),
    /// Move StructTag string value of the event type e.g. `0x2::devnet_nft::MintNFTEvent`
    MoveEventType(String),
    MoveEventField {
        path: String,
        value: Value,
    },
    SenderAddress(SuiAddress),
    EventType(EventType),
    ObjectId(ObjectID),
    All(Vec<SuiEventFilter>),
    Any(Vec<SuiEventFilter>),
    And(Box<SuiEventFilter>, Box<SuiEventFilter>),
    Or(Box<SuiEventFilter>, Box<SuiEventFilter>),
}

impl TryInto<EventFilter> for SuiEventFilter {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<EventFilter, anyhow::Error> {
        use SuiEventFilter::*;
        Ok(match self {
            Package(id) => EventFilter::Package(id),
            Module(module) => EventFilter::Module(Identifier::new(module)?),
            MoveEventType(event_type) => {
                // parse_sui_struct_tag converts StructTag string e.g. `0x2::devnet_nft::MintNFTEvent` to StructTag object
                EventFilter::MoveEventType(parse_sui_struct_tag(&event_type)?)
            }
            MoveEventField { path, value } => EventFilter::MoveEventField { path, value },
            SenderAddress(address) => EventFilter::SenderAddress(address),
            ObjectId(id) => EventFilter::ObjectId(id),
            All(filters) => EventFilter::MatchAll(
                filters
                    .into_iter()
                    .map(SuiEventFilter::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            Any(filters) => EventFilter::MatchAny(
                filters
                    .into_iter()
                    .map(SuiEventFilter::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            And(filter_a, filter_b) => All(vec![*filter_a, *filter_b]).try_into()?,
            Or(filter_a, filter_b) => Any(vec![*filter_a, *filter_b]).try_into()?,
            EventType(type_) => EventFilter::EventType(type_),
        })
    }
}
