// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! IndexStore supports creation of various ancillary indexes of state in SuiDataStore.
//! The main user of this data is the explorer.

use std::cmp::{max, min};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::anyhow;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{ModuleId, StructTag};
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

use sui_json_rpc_types::SuiObjectDataFilter;
use sui_types::base_types::{
    ObjectID, ObjectType, SuiAddress, TransactionDigest, TxSequenceNumber,
};
use sui_types::base_types::{ObjectInfo, ObjectRef};
use sui_types::digests::TransactionEventsDigest;
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldName};
use sui_types::error::{SuiError, SuiResult};
use sui_types::fp_ensure;
use sui_types::messages::TransactionEvents;
use sui_types::object::Owner;
use sui_types::query::TransactionFilter;
use typed_store::rocks::DBOptions;
use typed_store::rocks::{default_db_options, point_lookup_db_options, DBMap, MetricConf};
use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store_derive::DBMapUtils;

type OwnerIndexKey = (SuiAddress, ObjectID);
type DynamicFieldKey = (ObjectID, ObjectID);
type EventId = (TxSequenceNumber, usize);
type EventIndex = (TransactionEventsDigest, TransactionDigest, u64);

pub const MAX_TX_RANGE_SIZE: u64 = 4096;

pub const MAX_GET_OWNED_OBJECT_SIZE: usize = 256;

pub struct ObjectIndexChanges {
    pub deleted_owners: Vec<OwnerIndexKey>,
    pub deleted_dynamic_fields: Vec<DynamicFieldKey>,
    pub new_owners: Vec<(OwnerIndexKey, ObjectInfo)>,
    pub new_dynamic_fields: Vec<(DynamicFieldKey, DynamicFieldInfo)>,
}

#[derive(DBMapUtils)]
pub struct IndexStoreTables {
    /// Index from sui address to transactions initiated by that address.
    #[default_options_override_fn = "transactions_from_addr_table_default_config"]
    transactions_from_addr: DBMap<(SuiAddress, TxSequenceNumber), TransactionDigest>,

    /// Index from sui address to transactions that were sent to that address.
    #[default_options_override_fn = "transactions_to_addr_table_default_config"]
    transactions_to_addr: DBMap<(SuiAddress, TxSequenceNumber), TransactionDigest>,

    /// Index from object id to transactions that used that object id as input.
    #[default_options_override_fn = "transactions_by_input_object_id_table_default_config"]
    transactions_by_input_object_id: DBMap<(ObjectID, TxSequenceNumber), TransactionDigest>,

    /// Index from object id to transactions that modified/created that object id.
    #[default_options_override_fn = "transactions_by_mutated_object_id_table_default_config"]
    transactions_by_mutated_object_id: DBMap<(ObjectID, TxSequenceNumber), TransactionDigest>,

    /// Index from package id, module and function identifier to transactions that used that moce function call as input.
    #[default_options_override_fn = "transactions_by_move_function_table_default_config"]
    transactions_by_move_function:
        DBMap<(ObjectID, String, String, TxSequenceNumber), TransactionDigest>,

    /// This is a map between the transaction digest and its timestamp (UTC timestamp in
    /// **milliseconds** since epoch 1/1/1970). A transaction digest is subjectively time stamped
    /// on a node according to the local machine time, so it varies across nodes.
    /// The timestamping happens when the node sees a txn certificate for the first time.
    #[default_options_override_fn = "timestamps_table_default_config"]
    timestamps: DBMap<TransactionDigest, u64>,

    /// Ordering of all indexed transactions.
    #[default_options_override_fn = "transactions_order_table_default_config"]
    transaction_order: DBMap<TxSequenceNumber, TransactionDigest>,

    /// Index from transaction digest to sequence number.
    #[default_options_override_fn = "transactions_seq_table_default_config"]
    transactions_seq: DBMap<TransactionDigest, TxSequenceNumber>,

    /// This is an index of object references to currently existing objects, indexed by the
    /// composite key of the SuiAddress of their owner and the object ID of the object.
    /// This composite index allows an efficient iterator to list all objected currently owned
    /// by a specific user, and their object reference.
    #[default_options_override_fn = "owner_index_table_default_config"]
    owner_index: DBMap<OwnerIndexKey, ObjectInfo>,

    /// This is an index of object references to currently existing dynamic field object, indexed by the
    /// composite key of the object ID of their parent and the object ID of the dynamic field object.
    /// This composite index allows an efficient iterator to list all objects currently owned
    /// by a specific object, and their object reference.
    #[default_options_override_fn = "dynamic_field_index_table_default_config"]
    dynamic_field_index: DBMap<DynamicFieldKey, DynamicFieldInfo>,

    #[default_options_override_fn = "index_table_default_config"]
    event_order: DBMap<EventId, EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_move_module: DBMap<(ModuleId, EventId), EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_move_event: DBMap<(StructTag, EventId), EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_sender: DBMap<(SuiAddress, EventId), EventIndex>,
    #[default_options_override_fn = "index_table_default_config"]
    event_by_time: DBMap<(u64, EventId), EventIndex>,
}

pub struct IndexStore {
    next_sequence_number: AtomicU64,
    tables: IndexStoreTables,
}

// These functions are used to initialize the DB tables
fn transactions_order_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_seq_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_from_addr_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_to_addr_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_by_input_object_id_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_by_mutated_object_id_table_default_config() -> DBOptions {
    default_db_options()
}
fn transactions_by_move_function_table_default_config() -> DBOptions {
    default_db_options()
}
fn timestamps_table_default_config() -> DBOptions {
    point_lookup_db_options()
}
fn owner_index_table_default_config() -> DBOptions {
    default_db_options()
}
fn dynamic_field_index_table_default_config() -> DBOptions {
    default_db_options()
}
fn index_table_default_config() -> DBOptions {
    default_db_options()
}

impl IndexStore {
    pub fn new(path: PathBuf) -> Self {
        let tables =
            IndexStoreTables::open_tables_read_write(path, MetricConf::default(), None, None);
        let next_sequence_number = tables
            .transaction_order
            .iter()
            .skip_to_last()
            .next()
            .map(|(seq, _)| seq + 1)
            .unwrap_or(0)
            .into();

        Self {
            tables,
            next_sequence_number,
        }
    }

    pub fn index_tx(
        &self,
        sender: SuiAddress,
        active_inputs: impl Iterator<Item = ObjectID>,
        mutated_objects: impl Iterator<Item = (ObjectRef, Owner)> + Clone,
        move_functions: impl Iterator<Item = (ObjectID, Identifier, Identifier)> + Clone,
        events: &TransactionEvents,
        object_index_changes: ObjectIndexChanges,
        digest: &TransactionDigest,
        timestamp_ms: u64,
    ) -> SuiResult<u64> {
        let sequence = self.next_sequence_number.fetch_add(1, Ordering::SeqCst);

        let batch = self.tables.transactions_from_addr.batch();

        let batch = batch.insert_batch(
            &self.tables.transaction_order,
            std::iter::once((sequence, *digest)),
        )?;

        let batch = batch.insert_batch(
            &self.tables.transactions_seq,
            std::iter::once((*digest, sequence)),
        )?;

        let batch = batch.insert_batch(
            &self.tables.transactions_from_addr,
            std::iter::once(((sender, sequence), *digest)),
        )?;

        let batch = batch.insert_batch(
            &self.tables.transactions_by_input_object_id,
            active_inputs.map(|id| ((id, sequence), *digest)),
        )?;

        let batch = batch.insert_batch(
            &self.tables.transactions_by_mutated_object_id,
            mutated_objects
                .clone()
                .map(|(obj_ref, _)| ((obj_ref.0, sequence), *digest)),
        )?;

        let batch = batch.insert_batch(
            &self.tables.transactions_by_move_function,
            move_functions.map(|(obj_id, module, function)| {
                (
                    (obj_id, module.to_string(), function.to_string(), sequence),
                    *digest,
                )
            }),
        )?;

        let batch = batch.insert_batch(
            &self.tables.transactions_to_addr,
            mutated_objects.filter_map(|(_, owner)| {
                owner
                    .get_owner_address()
                    .ok()
                    .map(|addr| ((addr, sequence), digest))
            }),
        )?;

        let batch = batch.insert_batch(
            &self.tables.timestamps,
            std::iter::once((*digest, timestamp_ms)),
        )?;

        // Owner index
        let batch = batch.delete_batch(
            &self.tables.owner_index,
            object_index_changes.deleted_owners.into_iter(),
        )?;
        let batch = batch.delete_batch(
            &self.tables.dynamic_field_index,
            object_index_changes.deleted_dynamic_fields.into_iter(),
        )?;
        let batch = batch.insert_batch(
            &self.tables.owner_index,
            object_index_changes.new_owners.into_iter(),
        )?;
        let batch = batch.insert_batch(
            &self.tables.dynamic_field_index,
            object_index_changes.new_dynamic_fields.into_iter(),
        )?;

        // events
        let event_digest = events.digest();
        let batch = batch.insert_batch(
            &self.tables.event_order,
            events
                .data
                .iter()
                .enumerate()
                .map(|(i, _)| ((sequence, i), (event_digest, *digest, timestamp_ms))),
        )?;
        let batch = batch.insert_batch(
            &self.tables.event_by_move_module,
            events
                .data
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    (
                        i,
                        ModuleId::new(e.package_id.into(), e.transaction_module.clone()),
                    )
                })
                .map(|(i, m)| ((m, (sequence, i)), (event_digest, *digest, timestamp_ms))),
        )?;
        let batch = batch.insert_batch(
            &self.tables.event_by_sender,
            events.data.iter().enumerate().map(|(i, e)| {
                (
                    (e.sender, (sequence, i)),
                    (event_digest, *digest, timestamp_ms),
                )
            }),
        )?;
        let batch = batch.insert_batch(
            &self.tables.event_by_move_event,
            events.data.iter().enumerate().map(|(i, e)| {
                (
                    (e.type_.clone(), (sequence, i)),
                    (event_digest, *digest, timestamp_ms),
                )
            }),
        )?;

        let batch = batch.insert_batch(
            &self.tables.event_by_time,
            events.data.iter().enumerate().map(|(i, _)| {
                (
                    (timestamp_ms, (sequence, i)),
                    (event_digest, *digest, timestamp_ms),
                )
            }),
        )?;

        batch.write()?;

        Ok(sequence)
    }

    pub fn next_sequence_number(&self) -> TxSequenceNumber {
        self.next_sequence_number.load(Ordering::SeqCst) + 1
    }

    pub fn get_transactions(
        &self,
        filter: Option<TransactionFilter>,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        reverse: bool,
    ) -> Result<Vec<TransactionDigest>, anyhow::Error> {
        // Lookup TransactionDigest sequence number,
        let cursor = if let Some(cursor) = cursor {
            Some(
                self.get_transaction_seq(&cursor)?
                    .ok_or_else(|| anyhow!("Transaction [{cursor:?}] not found."))?,
            )
        } else {
            None
        };
        Ok(match filter {
            Some(TransactionFilter::MoveFunction {
                package,
                module,
                function,
            }) => self.get_transactions_by_move_function(
                package, module, function, cursor, limit, reverse,
            )?,
            Some(TransactionFilter::InputObject(object_id)) => {
                self.get_transactions_by_input_object(object_id, cursor, limit, reverse)?
            }
            Some(TransactionFilter::ChangedObject(object_id)) => {
                self.get_transactions_by_mutated_object(object_id, cursor, limit, reverse)?
            }
            Some(TransactionFilter::FromAddress(address)) => {
                self.get_transactions_from_addr(address, cursor, limit, reverse)?
            }
            Some(TransactionFilter::ToAddress(address)) => {
                self.get_transactions_to_addr(address, cursor, limit, reverse)?
            }
            None => {
                let iter = self.tables.transaction_order.iter();

                if reverse {
                    let iter = iter
                        .skip_prior_to(&cursor.unwrap_or(TxSequenceNumber::MAX))?
                        .reverse()
                        .skip(usize::from(cursor.is_some()))
                        .map(|(_, digest)| digest);
                    if let Some(limit) = limit {
                        iter.take(limit).collect()
                    } else {
                        iter.collect()
                    }
                } else {
                    let iter = iter
                        .skip_to(&cursor.unwrap_or(TxSequenceNumber::MIN))?
                        .skip(usize::from(cursor.is_some()))
                        .map(|(_, digest)| digest);
                    if let Some(limit) = limit {
                        iter.take(limit).collect()
                    } else {
                        iter.collect()
                    }
                }
            }
        })
    }

    pub fn get_transactions_in_range_deprecated(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            start <= end,
            SuiError::FullNodeInvalidTxRangeQuery {
                error: format!(
                    "start must not exceed end, (start={}, end={}) given",
                    start, end
                ),
            }
            .into()
        );
        fp_ensure!(
            end - start <= MAX_TX_RANGE_SIZE,
            SuiError::FullNodeInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE,
                    end - start
                ),
            }
            .into()
        );
        let res = self.transactions_in_seq_range(start, end)?;
        debug!(?start, ?end, ?res, "Fetched transactions");
        Ok(res)
    }

    fn transactions_in_seq_range(
        &self,
        start: TxSequenceNumber,
        end: TxSequenceNumber,
    ) -> SuiResult<Vec<(TxSequenceNumber, TransactionDigest)>> {
        Ok(self
            .tables
            .transaction_order
            .iter()
            .skip_to(&start)?
            .take_while(|(seq, _tx)| *seq < end)
            .collect())
    }

    pub fn get_recent_transactions(
        &self,
        count: u64,
    ) -> Result<Vec<(TxSequenceNumber, TransactionDigest)>, anyhow::Error> {
        fp_ensure!(
            count <= MAX_TX_RANGE_SIZE,
            SuiError::FullNodeInvalidTxRangeQuery {
                error: format!(
                    "Number of transactions queried must not exceed {}, {} queried",
                    MAX_TX_RANGE_SIZE, count
                ),
            }
            .into()
        );
        let end = self.next_sequence_number();
        let start = if end >= count { end - count } else { 0 };
        self.get_transactions_in_range_deprecated(start, end)
    }

    /// Returns unix timestamp for a transaction if it exists
    pub fn get_timestamp_ms(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<u64>> {
        let ts = self.tables.timestamps.get(transaction_digest)?;
        Ok(ts)
    }

    fn get_transactions_from_index<KeyT: Clone + Serialize + DeserializeOwned + PartialEq>(
        index: &DBMap<(KeyT, TxSequenceNumber), TransactionDigest>,
        key: KeyT,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Ok(if reverse {
            let iter = index
                .iter()
                .skip_prior_to(&(key.clone(), cursor.unwrap_or(TxSequenceNumber::MAX)))?
                .reverse()
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, _), _)| *id == key)
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        } else {
            let iter = index
                .iter()
                .skip_to(&(key.clone(), cursor.unwrap_or(TxSequenceNumber::MIN)))?
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, _), _)| *id == key)
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        })
    }

    pub fn get_transactions_by_input_object(
        &self,
        input_object: ObjectID,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_by_input_object_id,
            input_object,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transactions_by_mutated_object(
        &self,
        mutated_object: ObjectID,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_by_mutated_object_id,
            mutated_object,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_from_addr,
            addr,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        let cursor_val = cursor.unwrap_or(if reverse {
            TxSequenceNumber::MAX
        } else {
            TxSequenceNumber::MIN
        });

        let key = (
            package,
            module.clone().unwrap_or_default(),
            function.clone().unwrap_or_default(),
            cursor_val,
        );
        let iter = self.tables.transactions_by_move_function.iter();
        Ok(if reverse {
            let iter = iter
                .skip_prior_to(&key)?
                .reverse()
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, m, f, _), _)| {
                    *id == package
                        && module.as_ref().map(|x| x == m).unwrap_or(true)
                        && function.as_ref().map(|x| x == f).unwrap_or(true)
                })
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        } else {
            let iter = iter
                .skip_to(&key)?
                // skip one more if exclusive cursor is Some
                .skip(usize::from(cursor.is_some()))
                .take_while(|((id, m, f, _), _)| {
                    *id == package
                        && module.as_ref().map(|x| x == m).unwrap_or(true)
                        && function.as_ref().map(|x| x == f).unwrap_or(true)
                })
                .map(|(_, digest)| digest);
            if let Some(limit) = limit {
                iter.take(limit).collect()
            } else {
                iter.collect()
            }
        })
    }

    pub fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
        cursor: Option<TxSequenceNumber>,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Self::get_transactions_from_index(
            &self.tables.transactions_to_addr,
            addr,
            cursor,
            limit,
            reverse,
        )
    }

    pub fn get_transaction_seq(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<TxSequenceNumber>> {
        Ok(self.tables.transactions_seq.get(digest)?)
    }

    pub fn all_events(
        &self,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Ok(if descending {
            self.tables
                .event_order
                .iter()
                .skip_prior_to(&(tx_seq, event_seq))?
                .reverse()
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            self.tables
                .event_order
                .iter()
                .skip_to(&(tx_seq, event_seq))?
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    pub fn events_by_transaction(
        &self,
        digest: &TransactionDigest,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        let seq = self
            .get_transaction_seq(digest)?
            .ok_or(SuiError::TransactionNotFound { digest: *digest })?;
        Ok(if descending {
            self.tables
                .event_order
                .iter()
                .skip_prior_to(&(min(tx_seq, seq), event_seq))?
                .reverse()
                .take_while(|((tx, _), _)| tx == &seq)
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            self.tables
                .event_order
                .iter()
                .skip_to(&(max(tx_seq, seq), event_seq))?
                .take_while(|((tx, _), _)| tx == &seq)
                .take(limit)
                .map(|((_, event_seq), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    fn get_event_from_index<KeyT: Clone + PartialEq + Serialize + DeserializeOwned>(
        index: &DBMap<(KeyT, EventId), (TransactionEventsDigest, TransactionDigest, u64)>,
        key: &KeyT,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Ok(if descending {
            index
                .iter()
                .skip_prior_to(&(key.clone(), (tx_seq, event_seq)))?
                .reverse()
                .take_while(|((m, _), _)| m == key)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            index
                .iter()
                .skip_to(&(key.clone(), (tx_seq, event_seq)))?
                .take_while(|((m, _), _)| m == key)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    pub fn events_by_module_id(
        &self,
        module: &ModuleId,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Self::get_event_from_index(
            &self.tables.event_by_move_module,
            module,
            tx_seq,
            event_seq,
            limit,
            descending,
        )
    }

    pub fn events_by_move_event_struct_name(
        &self,
        struct_name: &StructTag,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Self::get_event_from_index(
            &self.tables.event_by_move_event,
            struct_name,
            tx_seq,
            event_seq,
            limit,
            descending,
        )
    }

    pub fn events_by_sender(
        &self,
        sender: &SuiAddress,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Self::get_event_from_index(
            &self.tables.event_by_sender,
            sender,
            tx_seq,
            event_seq,
            limit,
            descending,
        )
    }

    pub fn event_iterator(
        &self,
        start_time: u64,
        end_time: u64,
        tx_seq: TxSequenceNumber,
        event_seq: usize,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<(TransactionEventsDigest, TransactionDigest, usize, u64)>> {
        Ok(if descending {
            self.tables
                .event_by_time
                .iter()
                .skip_prior_to(&(end_time, (tx_seq, event_seq)))?
                .reverse()
                .take_while(|((m, _), _)| m >= &start_time)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        } else {
            self.tables
                .event_by_time
                .iter()
                .skip_to(&(start_time, (tx_seq, event_seq)))?
                .take_while(|((m, _), _)| m <= &end_time)
                .take(limit)
                .map(|((_, (_, event_seq)), (digest, tx_digest, time))| {
                    (digest, tx_digest, event_seq, time)
                })
                .collect()
        })
    }

    pub fn get_dynamic_fields_iterator(
        &self,
        object: ObjectID,
        cursor: Option<ObjectID>,
    ) -> SuiResult<impl Iterator<Item = DynamicFieldInfo> + '_> {
        debug!(?object, "get_dynamic_fields");
        Ok(self
            .tables
            .dynamic_field_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(object, cursor.unwrap_or(ObjectID::ZERO)))?
            // skip an extra b/c the cursor is exclusive
            .skip(usize::from(cursor.is_some()))
            .take_while(move |((object_owner, _), _)| (object_owner == &object))
            .map(|(_, object_info)| object_info))
    }

    pub fn get_dynamic_field_object_id(
        &self,
        object: ObjectID,
        name: &DynamicFieldName,
    ) -> SuiResult<Option<ObjectID>> {
        debug!(?object, "get_dynamic_field_object_id");
        Ok(self
            .tables
            .dynamic_field_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(object, ObjectID::ZERO))?
            .find(|((object_owner, _), info)| {
                object_owner == &object
                    && info.name.type_ == name.type_
                    && info.name.value == name.value
            })
            .map(|(_, object_info)| object_info.object_id))
    }

    pub fn get_owner_objects(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
        limit: usize,
        filter: Option<SuiObjectDataFilter>,
    ) -> SuiResult<Vec<ObjectInfo>> {
        let cursor = match cursor {
            Some(cursor) => cursor,
            None => ObjectID::ZERO,
        };

        Ok(self
            .get_owner_objects_iterator(owner, cursor, limit, filter)?
            .collect())
    }

    /// starting_object_id can be used to implement pagination, where a client remembers the last
    /// object id of each page, and use it to query the next page.
    pub fn get_owner_objects_iterator(
        &self,
        owner: SuiAddress,
        starting_object_id: ObjectID,
        count: usize,
        filter: Option<SuiObjectDataFilter>,
    ) -> SuiResult<impl Iterator<Item = ObjectInfo> + '_> {
        // We use +1 to grab the next cursor
        let count = min(count, MAX_GET_OWNED_OBJECT_SIZE + 1);
        debug!(?owner, ?count, ?starting_object_id, "get_owner_objects");
        Ok(self
            .tables
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(owner, starting_object_id))?
            .filter(move |((object_owner, _), obj_info)| {
                let to_include: bool = match &filter {
                    Some(SuiObjectDataFilter::StructType(struct_tag)) => {
                        let obj_tag: StructTag = match obj_info.type_.clone().try_into() {
                            Ok(tag) => tag,
                            Err(_) => {
                                return false;
                            }
                        };
                        // If people do not provide type_params, we will match all type_params
                        // e.g. `0x2::coin::Coin` can match `0x2::coin::Coin<0x2::sui::SUI>`
                        if !struct_tag.type_params.is_empty()
                            && struct_tag.type_params != obj_tag.type_params
                        {
                            return false;
                        }
                        return obj_tag.address == struct_tag.address
                            && obj_tag.module == struct_tag.module
                            && obj_tag.name == struct_tag.name;
                    }
                    Some(SuiObjectDataFilter::MoveModule { package, module }) => {
                        if let ObjectType::Struct(o) = obj_info.clone().type_ {
                            o.address().to_string() == package.to_string()
                                && o.module().to_string() == module.to_string()
                        } else {
                            false
                        }
                    }
                    Some(SuiObjectDataFilter::Package(package_id)) => {
                        if let ObjectType::Struct(o) = obj_info.clone().type_ {
                            o.address().to_string() == package_id.to_string()
                        } else {
                            false
                        }
                    }
                    None => true,
                };
                object_owner == &owner && to_include
            })
            .take(count)
            .map(|(_, object_info)| object_info))
    }

    pub fn insert_genesis_objects(&self, object_index_changes: ObjectIndexChanges) -> SuiResult {
        let batch = self.tables.owner_index.batch();
        let batch = batch.insert_batch(
            &self.tables.owner_index,
            object_index_changes.new_owners.into_iter(),
        )?;
        let batch = batch.insert_batch(
            &self.tables.dynamic_field_index,
            object_index_changes.new_dynamic_fields.into_iter(),
        )?;
        batch.write()?;
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.tables.owner_index.is_empty()
    }
}
