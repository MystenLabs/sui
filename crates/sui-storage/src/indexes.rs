// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! IndexStore supports creation of various ancillary indexes of state in SuiDataStore.
//! The main user of this data is the explorer.

use move_core_types::identifier::Identifier;
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::base_types::{ObjectInfo, ObjectRef};
use sui_types::batch::TxSequenceNumber;
use sui_types::dynamic_field::DynamicFieldInfo;
use sui_types::error::SuiResult;
use sui_types::object::Owner;
use typed_store::rocks::DBMap;
use typed_store::rocks::DBOptions;
use typed_store::traits::Map;
use typed_store::traits::TypedStoreDebug;
use typed_store_derive::DBMapUtils;

use crate::default_db_options;

type OwnerIndexKey = (SuiAddress, ObjectID);
type DynamicFieldKey = (ObjectID, ObjectID);

pub struct ObjectIndexChanges {
    pub deleted_owners: Vec<OwnerIndexKey>,
    pub deleted_dynamic_fields: Vec<DynamicFieldKey>,
    pub new_owners: Vec<(OwnerIndexKey, ObjectInfo)>,
    pub new_dynamic_fields: Vec<(DynamicFieldKey, DynamicFieldInfo)>,
}

#[derive(DBMapUtils)]
pub struct IndexStore {
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

// These functions are used to initialize the DB tables
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
    pub fn index_tx(
        &self,
        sender: SuiAddress,
        active_inputs: impl Iterator<Item = ObjectID>,
        mutated_objects: impl Iterator<Item = (ObjectRef, Owner)> + Clone,
        move_functions: impl Iterator<Item = (ObjectID, Identifier, Identifier)> + Clone,
        object_index_changes: ObjectIndexChanges,
        sequence: TxSequenceNumber,
        digest: &TransactionDigest,
        timestamp_ms: u64,
    ) -> SuiResult {
        let batch = self.transactions_from_addr.batch();

        let batch =
            batch.insert_batch(&self.transactions_seq, std::iter::once((*digest, sequence)))?;

        let batch = batch.insert_batch(
            &self.transactions_from_addr,
            std::iter::once(((sender, sequence), *digest)),
        )?;

        let batch = batch.insert_batch(
            &self.transactions_by_input_object_id,
            active_inputs.map(|id| ((id, sequence), *digest)),
        )?;

        let batch = batch.insert_batch(
            &self.transactions_by_mutated_object_id,
            mutated_objects
                .clone()
                .map(|(obj_ref, _)| ((obj_ref.0, sequence), *digest)),
        )?;

        let batch = batch.insert_batch(
            &self.transactions_by_move_function,
            move_functions.map(|(obj_id, module, function)| {
                (
                    (obj_id, module.to_string(), function.to_string(), sequence),
                    *digest,
                )
            }),
        )?;

        let batch = batch.insert_batch(
            &self.transactions_to_addr,
            mutated_objects.filter_map(|(_, owner)| {
                owner
                    .get_owner_address()
                    .ok()
                    .map(|addr| ((addr, sequence), digest))
            }),
        )?;

        let batch =
            batch.insert_batch(&self.timestamps, std::iter::once((*digest, timestamp_ms)))?;

        // Owner index
        let batch = batch.delete_batch(
            &self.owner_index,
            object_index_changes.deleted_owners.into_iter(),
        )?;
        let batch = batch.delete_batch(
            &self.dynamic_field_index,
            object_index_changes.deleted_dynamic_fields.into_iter(),
        )?;
        let batch = batch.insert_batch(
            &self.owner_index,
            object_index_changes.new_owners.into_iter(),
        )?;
        let batch = batch.insert_batch(
            &self.dynamic_field_index,
            object_index_changes.new_dynamic_fields.into_iter(),
        )?;

        batch.write()?;

        Ok(())
    }

    /// Returns unix timestamp for a transaction if it exists
    pub fn get_timestamp_ms(
        &self,
        transaction_digest: &TransactionDigest,
    ) -> SuiResult<Option<u64>> {
        let ts = self.timestamps.get(transaction_digest)?;
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
            &self.transactions_by_input_object_id,
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
            &self.transactions_by_mutated_object_id,
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
            &self.transactions_from_addr,
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
        let iter = self.transactions_by_move_function.iter();
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
        Self::get_transactions_from_index(&self.transactions_to_addr, addr, cursor, limit, reverse)
    }

    pub fn get_transaction_seq(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<TxSequenceNumber>> {
        Ok(self.transactions_seq.get(digest)?)
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
        name: &str,
    ) -> SuiResult<Option<ObjectID>> {
        debug!(?object, "get_dynamic_field_object_id");
        Ok(self
            .dynamic_field_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(object, ObjectID::ZERO))?
            .find(|((object_owner, _), info)| (object_owner == &object && info.name == name))
            .map(|(_, object_info)| object_info.object_id))
    }

    pub fn get_owner_objects(&self, owner: SuiAddress) -> SuiResult<Vec<ObjectInfo>> {
        debug!(?owner, "get_owner_objects");
        Ok(self
            .owner_index
            .iter()
            // The object id 0 is the smallest possible
            .skip_to(&(owner, ObjectID::ZERO))?
            .take_while(|((object_owner, _), _)| (object_owner == &owner))
            .map(|(_, object_info)| object_info)
            .collect())
    }
}
