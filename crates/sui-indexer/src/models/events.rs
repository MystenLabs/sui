// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::events;
use diesel::prelude::*;
use move_core_types::identifier::Identifier;
use move_core_types::parser::parse_struct_tag;
use serde_json::Value;
use std::str::FromStr;
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::TransactionDigest;
use sui_types::event::EventID;

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
    pub parsed_json: Value,
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
            parsed_json: se.parsed_json,
            event_bcs: se.bcs,
        }
    }
}

impl TryInto<SuiEvent> for Event {
    type Error = IndexerError;
    fn try_into(self) -> Result<SuiEvent, Self::Error> {
        // Event in this table is always MoveEvent
        let package_id = self.package.parse().map_err(|e| {
            IndexerError::SerdeError(format!("Failed to parse event package ID: {:?}", e))
        })?;
        let sender = self.sender.parse().map_err(|e| {
            IndexerError::SerdeError(format!("Failed to parse event sender address: {:?}", e))
        })?;
        Ok(SuiEvent {
            id: EventID {
                tx_digest: TransactionDigest::from_str(&self.transaction_digest)?,
                event_seq: self.event_sequence as u64,
            },
            package_id,
            transaction_module: Identifier::from_str(&self.module)?,
            sender,
            type_: parse_struct_tag(&self.event_type)?,
            bcs: self.event_bcs,
            parsed_json: self.parsed_json,
            timestamp_ms: self.event_time_ms.map(|t| t as u64),
        })
    }
}
