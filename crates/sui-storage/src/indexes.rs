// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! IndexStore supports creation of various ancillary indexes of state in SuiDataStore.
//! The main user of this data is the explorer.

use anyhow::anyhow;
use move_core_types::identifier::Identifier;
use serde::{de::DeserializeOwned, Serialize};
use std::cmp::min;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::debug;
use typed_store::rocks::DBOptions;
use typed_store::rocks::{DBMap, MetricConf};
use typed_store::traits::Map;
use typed_store::traits::{TableSummary, TypedStoreDebug};
use typed_store_derive::DBMapUtils;

use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest, TxSequenceNumber};
use sui_types::base_types::{ObjectInfo, ObjectRef};
use sui_types::dynamic_field::{DynamicFieldInfo, DynamicFieldName};
use sui_types::error::{SuiError, SuiResult};
use sui_types::fp_ensure;
use sui_types::object::Owner;
use sui_types::query::TransactionQuery;

use crate::default_db_options;

type OwnerIndexKey = (SuiAddress, ObjectID);
type DynamicFieldKey = (ObjectID, ObjectID);

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
}

pub struct IndexStore {
    next_sequence_number: AtomicU64,
    tables: IndexStoreTables,
}

// These functions are used to initialize the DB tables
fn transactions_order_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_seq_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_from_addr_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_to_addr_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_by_input_object_id_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_by_mutated_object_id_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_by_move_function_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn timestamps_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).1
}
fn owner_index_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
}
fn dynamic_field_index_table_default_config() -> DBOptions {
    default_db_options(None, Some(1_000_000)).0
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

        batch.write()?;

        Ok(sequence)
    }

    pub fn next_sequence_number(&self) -> TxSequenceNumber {
        self.next_sequence_number.load(Ordering::SeqCst) + 1
    }

    pub fn get_transactions(
        &self,
        query: TransactionQuery,
        cursor: Option<TransactionDigest>,
        limit: Option<usize>,
        reverse: bool,
    ) -> Result<Vec<TransactionDigest>, anyhow::Error> {
        // Lookup TransactionDigest sequence number,
        // also default cursor to 0 or the current sequence number depends on ordering.
        let cursor = if let Some(cursor) = cursor {
            self.get_transaction_seq(&cursor)?
                .ok_or_else(|| anyhow!("Transaction [{cursor:?}] not found."))?
        } else if reverse {
            TxSequenceNumber::MAX
        } else {
            TxSequenceNumber::MIN
        };

        Ok(match query {
            TransactionQuery::MoveFunction {
                package,
                module,
                function,
            } => self.get_transactions_by_move_function(
                package, module, function, cursor, limit, reverse,
            )?,
            TransactionQuery::InputObject(object_id) => {
                self.get_transactions_by_input_object(object_id, cursor, limit, reverse)?
            }
            TransactionQuery::MutatedObject(object_id) => {
                self.get_transactions_by_mutated_object(object_id, cursor, limit, reverse)?
            }
            TransactionQuery::FromAddress(address) => {
                self.get_transactions_from_addr(address, cursor, limit, reverse)?
            }
            TransactionQuery::ToAddress(address) => {
                self.get_transactions_to_addr(address, cursor, limit, reverse)?
            }
            TransactionQuery::All => {
                let iter = self.tables.transaction_order.iter();

                if reverse {
                    let iter = iter
                        .skip_prior_to(&cursor)?
                        .reverse()
                        .map(|(_, digest)| digest);
                    if let Some(limit) = limit {
                        iter.take(limit).collect()
                    } else {
                        iter.collect()
                    }
                } else {
                    let iter = iter.skip_to(&cursor)?.map(|(_, digest)| digest);
                    if let Some(limit) = limit {
                        iter.take(limit).collect()
                    } else {
                        iter.collect()
                    }
                }
            }
        })
    }

    pub fn get_transactions_in_range(
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
        self.get_transactions_in_range(start, end)
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
        cursor: TxSequenceNumber,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        Ok(if reverse {
            let iter = index
                .iter()
                .skip_prior_to(&(key.clone(), cursor))?
                .reverse()
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
                .skip_to(&(key.clone(), cursor))?
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
        cursor: TxSequenceNumber,
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
        cursor: TxSequenceNumber,
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
        cursor: TxSequenceNumber,
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
        cursor: TxSequenceNumber,
        limit: Option<usize>,
        reverse: bool,
    ) -> SuiResult<Vec<TransactionDigest>> {
        let key = (
            package,
            module.clone().unwrap_or_default(),
            function.clone().unwrap_or_default(),
            cursor,
        );
        let iter = self.tables.transactions_by_move_function.iter();
        Ok(if reverse {
            let iter = iter
                .skip_prior_to(&key)?
                .reverse()
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
        cursor: TxSequenceNumber,
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

    pub fn get_dynamic_fields(
        &self,
        object: ObjectID,
        cursor: Option<ObjectID>,
        limit: usize,
    ) -> SuiResult<Vec<DynamicFieldInfo>> {
        debug!(?object, "get_dynamic_fields");
        let cursor = cursor.unwrap_or(ObjectID::ZERO);
        Ok(self
            .tables
            .dynamic_field_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(object, cursor))?
            .take_while(|((object_owner, _), _)| (object_owner == &object))
            .map(|(_, object_info)| object_info)
            .take(limit)
            .collect())
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

    pub fn get_owner_objects(&self, owner: SuiAddress) -> SuiResult<Vec<ObjectInfo>> {
        Ok(self
            .get_owner_objects_iterator(owner, ObjectID::ZERO, MAX_GET_OWNED_OBJECT_SIZE)?
            .collect())
    }

    /// starting_object_id can be used to implement pagination, where a client remembers the last
    /// object id of each page, and use it to query the next page.
    pub fn get_owner_objects_iterator(
        &self,
        owner: SuiAddress,
        starting_object_id: ObjectID,
        count: usize,
    ) -> SuiResult<impl Iterator<Item = ObjectInfo> + '_> {
        let count = min(count, MAX_GET_OWNED_OBJECT_SIZE);
        debug!(?owner, ?count, ?starting_object_id, "get_owner_objects");
        Ok(self
            .tables
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(owner, starting_object_id))?
            .take_while(move |((object_owner, _), _)| (object_owner == &owner))
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
