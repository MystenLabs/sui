// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use diesel::prelude::*;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::annotated_value::MoveStruct;
use move_core_types::identifier::Identifier;

use sui_json_rpc_types::{SuiEvent, SuiMoveStruct};
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::digests::TransactionDigest;
use sui_types::event::EventID;
use sui_types::object::MoveObject;
use sui_types::parse_sui_struct_tag;

use crate::errors::IndexerError;
use crate::schema::events;
use crate::types::IndexedEvent;

#[derive(Queryable, QueryableByName, Insertable, Debug, Clone)]
#[diesel(table_name = events)]
pub struct StoredEvent {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub tx_sequence_number: i64,

    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub event_sequence_number: i64,

    #[diesel(sql_type = diesel::sql_types::Bytea)]
    pub transaction_digest: Vec<u8>,

    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub checkpoint_sequence_number: i64,

    #[diesel(sql_type = diesel::sql_types::Array<diesel::sql_types::Nullable<diesel::pg::sql_types::Bytea>>)]
    pub senders: Vec<Option<Vec<u8>>>,

    #[diesel(sql_type = diesel::sql_types::Bytea)]
    pub package: Vec<u8>,

    #[diesel(sql_type = diesel::sql_types::Text)]
    pub module: String,

    #[diesel(sql_type = diesel::sql_types::Text)]
    pub event_type: String,

    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub timestamp_ms: i64,

    #[diesel(sql_type = diesel::sql_types::Bytea)]
    pub bcs: Vec<u8>,
}

impl From<IndexedEvent> for StoredEvent {
    fn from(event: IndexedEvent) -> Self {
        Self {
            tx_sequence_number: event.tx_sequence_number as i64,
            event_sequence_number: event.event_sequence_number as i64,
            transaction_digest: event.transaction_digest.into_inner().to_vec(),
            checkpoint_sequence_number: event.checkpoint_sequence_number as i64,
            senders: event
                .senders
                .into_iter()
                .map(|sender| Some(sender.to_vec()))
                .collect(),
            package: event.package.to_vec(),
            module: event.module.clone(),
            event_type: event.event_type.clone(),
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
        let package_id = ObjectID::from_bytes(self.package.clone()).map_err(|_e| {
            IndexerError::PersistentStorageDataCorruptionError(format!(
                "Failed to parse event package ID: {:?}",
                self.package
            ))
        })?;
        // Note: SuiEvent only has one sender today, so we always use the first one.
        let sender = self.senders.first().ok_or_else(|| {
            IndexerError::PersistentStorageDataCorruptionError(
                "Event senders should contain at least one address".to_string(),
            )
        })?;
        let sender = match sender {
            Some(s) => SuiAddress::from_bytes(s).map_err(|_e| {
                IndexerError::PersistentStorageDataCorruptionError(format!(
                    "Failed to parse event sender address: {:?}",
                    sender
                ))
            })?,
            None => {
                return Err(IndexerError::PersistentStorageDataCorruptionError(
                    "Event senders element should not be null".to_string(),
                ))
            }
        };

        let type_ = parse_sui_struct_tag(&self.event_type)?;

        let layout = MoveObject::get_layout_from_struct_tag(type_.clone(), module_cache)?;
        let move_object = MoveStruct::simple_deserialize(&self.bcs, &layout)
            .map_err(|e| IndexerError::SerdeError(e.to_string()))?;
        let parsed_json = SuiMoveStruct::from(move_object).to_json_value();
        let tx_digest =
            TransactionDigest::try_from(self.transaction_digest.as_slice()).map_err(|e| {
                IndexerError::SerdeError(format!(
                    "Failed to parse transaction digest: {:?}, error: {}",
                    self.transaction_digest, e
                ))
            })?;
        Ok(SuiEvent {
            id: EventID {
                tx_digest,
                event_seq: self.event_sequence_number as u64,
            },
            package_id,
            transaction_module: Identifier::from_str(&self.module)?,
            sender,
            type_,
            bcs: self.bcs,
            parsed_json,
            timestamp_ms: Some(self.timestamp_ms as u64),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
    use sui_types::event::Event;

    #[test]
    fn test_canonical_string_of_event_type() {
        let tx_digest = TransactionDigest::default();
        let event = Event {
            package_id: ObjectID::random(),
            transaction_module: Identifier::new("test").unwrap(),
            sender: AccountAddress::random().into(),
            type_: StructTag {
                address: AccountAddress::TWO,
                module: Identifier::new("test").unwrap(),
                name: Identifier::new("test").unwrap(),
                type_params: vec![],
            },
            contents: vec![],
        };

        let indexed_event = IndexedEvent::from_event(1, 1, 1, tx_digest, &event, 100);

        let stored_event = StoredEvent::from(indexed_event);

        assert_eq!(
            stored_event.event_type,
            "0x0000000000000000000000000000000000000000000000000000000000000002::test::test"
        );
    }
}
