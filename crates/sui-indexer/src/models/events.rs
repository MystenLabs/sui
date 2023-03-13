// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::events;
use diesel::prelude::*;
use sui_json_rpc_types::{SuiEvent, SuiEventEnvelope, SuiMoveStruct};
use sui_types::base_types::TransactionDigest;

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
    pub move_struct_bcs: String,
    pub event_bcs: Vec<u8>,
}

pub fn compose_event(
    sui_event: &SuiEvent,
    transaction_digest: String,
    seq: usize,
    timestamp_ms: Option<i64>,
) -> Option<Result<Event, IndexerError>> {
    match sui_event {
        SuiEvent::MoveEvent {
            package_id,
            transaction_module,
            sender,
            type_,
            fields,
            bcs,
        } => {
            let move_struct_bcs = serde_json::to_string(fields).map_err(|e| {
                IndexerError::SerdeError(format!("Failed to serialize MoveStruct to BCS: {}", e))
            });
            match move_struct_bcs {
                Ok(move_struct_bcs) => {
                    let event = Event {
                        id: None,
                        transaction_digest,
                        event_sequence: seq as i64,
                        sender: sender.to_string(),
                        package: package_id.to_string(),
                        module: transaction_module.clone(),
                        event_type: type_.clone(),
                        event_time_ms: timestamp_ms,
                        move_struct_bcs,
                        event_bcs: bcs.clone(),
                    };
                    Some(Ok(event))
                }
                Err(e) => Some(Err(e)),
            }
        }
        _ => None,
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
        let move_structs: Option<SuiMoveStruct> =
            // TODO(gegaowp): replace JSON encoding with BCS encoding.
            serde_json::from_str(self.move_struct_bcs.as_str()).map_err(|e| {
                IndexerError::SerdeError(format!(
                    "Failed to deserialize MoveStruct from BCS: {}",
                    e
                ))
            })?;
        Ok(SuiEvent::MoveEvent {
            package_id,
            transaction_module: self.module,
            sender,
            type_: self.event_type,
            fields: move_structs,
            bcs: self.event_bcs,
        })
    }
}

impl TryInto<SuiEventEnvelope> for Event {
    type Error = IndexerError;

    fn try_into(self) -> Result<SuiEventEnvelope, Self::Error> {
        let tx_digest: TransactionDigest = self.transaction_digest.parse().map_err(|e| {
            IndexerError::SerdeError(format!("Failed to parse event tx digest: {:?}", e))
        })?;
        let event_id = (tx_digest, self.event_sequence).into();
        let sui_event = self.clone().try_into()?;
        // timestamp should always exist b/c it's the checkpoint timestamp,
        // and indexer always reads after checkpoint is available on FN.
        let timestamp = self.event_time_ms.ok_or_else(|| {
            IndexerError::PostgresReadError("Timestamp is None in events table".to_string())
        })?;

        Ok(SuiEventEnvelope {
            timestamp: timestamp as u64,
            tx_digest,
            id: event_id,
            event: sui_event,
        })
    }
}
