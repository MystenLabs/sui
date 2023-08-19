// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use diesel::prelude::*;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::identifier::Identifier;
use move_core_types::value::MoveStruct;

use sui_json_rpc_types::{SuiEvent, SuiMoveStruct};
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::event::EventID;
use sui_types::object::{MoveObject, ObjectFormatOptions};
use sui_types::parse_sui_struct_tag;

use crate::errors::IndexerError;
use crate::schema::events;

#[derive(Queryable, Insertable, Debug, Clone)]
#[diesel(table_name = events)]
pub struct StoredEvent {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub transaction_digest: Vec<u8>,
    pub sender: Vec<u8>,
    pub package: Vec<u8>,
    pub module: String,
    pub event_type: String,
    pub bcs: Vec<u8>,
    pub timestamp_ms: i64,
}

impl From<&IndexedEvent> for StoredEvent {
    fn from(event: &IndexedEvent) -> Self {
        Self {
            tx_sequence_number: event.tx_sequence_number as i64,
            event_sequence_number: event.event_sequence_number as i64,
            transaction_digest: event.transaction_digest.into_inner().to_vec(),
            sender: event.sender.to_vec(),
            package: event.package.to_vec(),
            module: event.module,
            event_type: event.event_type,
            bcs: event.bcs.clone(),
            timestamp_ms: event.timestamp_ms as i64,
        }
    }
}

impl StoredEvent {
    pub fn try_into_sui_event(
        self,
        module_cache: &impl GetModule,
    ) -> Result<SuiEvent, IndexerError> {
        // Event in this table is always MoveEvent
        let package_id = ObjectID::from_bytes(self.package).map_err(|_e| {
            IndexerError::DataTransformationError(format!(
                "Failed to parse event package ID: {:?}",
                self.package
            ))
        })?;
        let sender = SuiAddress::from_bytes(self.sender).map_err(|_e| {
            IndexerError::DataTransformationError(format!(
                "Failed to parse event sender address: {:?}",
                self.sender
            ))
        })?;

        let type_ = parse_sui_struct_tag(&self.event_type)?;

        let layout = MoveObject::get_layout_from_struct_tag(
            type_.clone(),
            ObjectFormatOptions::default(),
            module_cache,
        )?;
        let move_object = MoveStruct::simple_deserialize(&self.bcs, &layout)
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

#[derive(Debug)]
pub struct IndexedEvent {
    pub tx_sequence_number: u64,
    pub event_sequence_number: u64,
    pub transaction_digest: TransactionDigest,
    pub sender: SuiAddress,
    pub package: ObjectID,
    pub module: String,
    pub event_type: String,
    pub bcs: Vec<u8>,
    pub timestamp_ms: u64,
}

impl IndexedEvent {
    // pub fn from_sui_event(tx_sequence_number: u64, se: SuiEvent, timestamp_ms: u64) -> Self {
    //     Self {
    //         tx_sequence_number: tx_sequence_number as i64,
    //         event_sequence_number: se.id.event_seq as i64,
    //         transaction_digest: se.id.tx_digest.into_inner().to_vec(),
    //         sender: se.sender.to_vec(),
    //         package: se.package_id.to_vec(),
    //         module: se.transaction_module.to_string(),
    //         event_type: se.type_.to_string(),
    //         timestamp_ms: timestamp_ms as i64,
    //         bcs: se.bcs,
    //     }
    // }

    pub fn from_event(
        tx_sequence_number: u64,
        event_sequence_number: u64,
        transaction_digest: TransactionDigest,
        event: &sui_types::event::Event,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            tx_sequence_number,
            event_sequence_number,
            transaction_digest,
            sender: event.sender,
            package: event.package_id,
            module: event.transaction_module.to_string(),
            event_type: event.type_.to_string(),
            bcs: event.contents.clone(),
            timestamp_ms,
        }
    }
}
