// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::sui_serde::BigInt;
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
use std::str::FromStr;

use crate::base_types::{ObjectID, SuiAddress, TransactionDigest};
use crate::error::{SuiError, SuiResult};
use crate::object::{MoveObject, ObjectFormatOptions};

/// A universal Sui event type encapsulating different types of events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// UTC timestamp in milliseconds since epoch (1/1/1970)
    pub timestamp: u64,
    /// Transaction digest of associated transaction
    pub tx_digest: TransactionDigest,
    /// Consecutive per-tx counter assigned to this event.
    pub event_num: u64,
    /// Specific event type
    pub event: Event,
    /// Move event's json value
    pub parsed_json: Value,
}
/// Unique ID of a Sui Event, the ID is a combination of tx seq number and event seq number,
/// the ID is local to this particular fullnode and will be different from other fullnode.
#[serde_as]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventID {
    pub tx_digest: TransactionDigest,
    #[schemars(with = "BigInt<u64>")]
    #[serde_as(as = "BigInt<u64>")]
    pub event_seq: u64,
}

impl From<(TransactionDigest, u64)> for EventID {
    fn from((tx_digest_num, event_seq_number): (TransactionDigest, u64)) -> Self {
        Self {
            tx_digest: tx_digest_num as TransactionDigest,
            event_seq: event_seq_number,
        }
    }
}

impl From<EventID> for String {
    fn from(id: EventID) -> Self {
        format!("{:?}:{}", id.tx_digest, id.event_seq)
    }
}

impl TryFrom<String> for EventID {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let values = value.split(':').collect::<Vec<_>>();
        ensure!(values.len() == 2, "Malformed EventID : {value}");
        Ok((
            TransactionDigest::from_str(values[0])?,
            u64::from_str(values[1])?,
        )
            .into())
    }
}

impl EventEnvelope {
    pub fn new(
        timestamp: u64,
        tx_digest: TransactionDigest,
        event_num: u64,
        event: Event,
        move_struct_json_value: Value,
    ) -> Self {
        Self {
            timestamp,
            tx_digest,
            event_num,
            event,
            parsed_json: move_struct_json_value,
        }
    }
}

/// Specific type of event
#[serde_as]
#[derive(PartialEq, Eq, Debug, Clone, Deserialize, Serialize)]
pub struct Event {
    pub package_id: ObjectID,
    pub transaction_module: Identifier,
    pub sender: SuiAddress,
    pub type_: StructTag,
    #[serde_as(as = "Bytes")]
    pub contents: Vec<u8>,
}

impl Event {
    pub fn new(
        package_id: &AccountAddress,
        module: &IdentStr,
        sender: SuiAddress,
        type_: StructTag,
        contents: Vec<u8>,
    ) -> Self {
        Self {
            package_id: ObjectID::from(*package_id),
            transaction_module: Identifier::from(module),
            sender,
            type_,
            contents,
        }
    }
    pub fn move_event_to_move_struct(
        type_: &StructTag,
        contents: &[u8],
        resolver: &impl GetModule,
    ) -> SuiResult<MoveStruct> {
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
