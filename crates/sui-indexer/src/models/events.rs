// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use diesel::prelude::*;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::identifier::Identifier;
use move_core_types::value::MoveStruct;

use sui_json_rpc_types::{SuiEvent, SuiMoveStruct};
use sui_types::base_types::TransactionDigest;
use sui_types::event::EventID;
use sui_types::object::{MoveObject, ObjectFormatOptions};
use sui_types::parse_sui_struct_tag;

use crate::errors::IndexerError;
use crate::schema::events;

#[derive(Queryable, Insertable, Debug, Clone)]
#[diesel(table_name = events)]
pub struct Event {
    #[diesel(deserialize_as = i64)]
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub event_sequence: i64,
    pub sender: String,
    pub package: String,
    pub module: String,
    pub event_type: String,
    pub event_time_ms: Option<i64>,
    pub event_bcs: Vec<u8>,
}

impl From<SuiEvent> for Event {
    fn from(se: SuiEvent) -> Self {
        Self {
            id: None,
            transaction_digest: se.id.tx_digest.base58_encode(),
            event_sequence: se.id.event_seq as i64,
            sender: se.sender.to_string(),
            package: se.package_id.to_string(),
            module: se.transaction_module.to_string(),
            event_type: se.type_.to_string(),
            event_time_ms: se.timestamp_ms.map(|t| t as i64),
            event_bcs: se.bcs,
        }
    }
}

impl Event {
    pub fn try_into(self, module_cache: &impl GetModule) -> Result<SuiEvent, IndexerError> {
        // Event in this table is always MoveEvent
        let package_id = self.package.parse().map_err(|e| {
            IndexerError::SerdeError(format!("Failed to parse event package ID: {:?}", e))
        })?;
        let sender = self.sender.parse().map_err(|e| {
            IndexerError::SerdeError(format!("Failed to parse event sender address: {:?}", e))
        })?;

        let type_ = parse_sui_struct_tag(&self.event_type)?;

        let layout = MoveObject::get_layout_from_struct_tag(
            type_.clone(),
            ObjectFormatOptions::default(),
            module_cache,
        )?;
        let move_object = MoveStruct::simple_deserialize(&self.event_bcs, &layout)
            .map_err(|e| IndexerError::SerdeError(e.to_string()))?;
        let parsed_json = SuiMoveStruct::from(move_object).to_json_value();

        Ok(SuiEvent {
            id: EventID {
                tx_digest: TransactionDigest::from_str(&self.transaction_digest)?,
                event_seq: self.event_sequence as u64,
            },
            package_id,
            transaction_module: Identifier::from_str(&self.module)?,
            sender,
            type_,
            bcs: self.event_bcs,
            parsed_json,
            timestamp_ms: self.event_time_ms.map(|t| t as u64),
        })
    }
}
