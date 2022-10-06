// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! IndexStore supports creation of various ancillary indexes of state in SuiDataStore.
//! The main user of this data is the explorer.

use rocksdb::Options;
use serde::{de::DeserializeOwned, Serialize};
use typed_store_derive::DBMapUtils;

use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::batch::TxSequenceNumber;
use sui_types::error::SuiResult;

use sui_types::base_types::ObjectRef;
use sui_types::object::Owner;

use move_core_types::identifier::Identifier;
use typed_store::rocks::DBMap;
use typed_store::traits::Map;
use typed_store::traits::TypedStoreDebug;

use crate::default_db_options;

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
}

// These functions are used to initialize the DB tables
fn transactions_from_addr_table_default_config() -> Options {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_to_addr_table_default_config() -> Options {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_by_input_object_id_table_default_config() -> Options {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_by_mutated_object_id_table_default_config() -> Options {
    default_db_options(None, Some(1_000_000)).0
}
fn transactions_by_move_function_table_default_config() -> Options {
    default_db_options(None, Some(1_000_000)).0
}
fn timestamps_table_default_config() -> Options {
    default_db_options(None, Some(1_000_000)).1
}

impl IndexStore {
    pub fn index_tx(
        &self,
        sender: SuiAddress,
        active_inputs: impl Iterator<Item = ObjectID>,
        mutated_objects: impl Iterator<Item = (ObjectRef, Owner)> + Clone,
        move_functions: impl Iterator<Item = (ObjectID, Identifier, Identifier)> + Clone,
        sequence: TxSequenceNumber,
        digest: &TransactionDigest,
        timestamp_ms: u64,
    ) -> SuiResult {
        let batch = self.transactions_from_addr.batch();

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

    fn get_transactions_by_object<
        KeyT: Clone + Serialize + DeserializeOwned + std::cmp::PartialEq,
    >(
        index: &DBMap<(KeyT, TxSequenceNumber), TransactionDigest>,
        object_id: KeyT,
    ) -> SuiResult<Vec<(TxSequenceNumber, TransactionDigest)>> {
        Ok(index
            .iter()
            .skip_to(&(object_id.clone(), TxSequenceNumber::MIN))?
            .take_while(|((id, _), _)| *id == object_id)
            .map(|((_, seq), digest)| (seq, digest))
            .collect())
    }

    pub fn get_transactions_by_input_object(
        &self,
        input_object: ObjectID,
    ) -> SuiResult<Vec<(TxSequenceNumber, TransactionDigest)>> {
        Self::get_transactions_by_object(&self.transactions_by_input_object_id, input_object)
    }

    pub fn get_transactions_by_mutated_object(
        &self,
        mutated_object: ObjectID,
    ) -> SuiResult<Vec<(TxSequenceNumber, TransactionDigest)>> {
        Self::get_transactions_by_object(&self.transactions_by_mutated_object_id, mutated_object)
    }

    pub fn get_transactions_from_addr(
        &self,
        addr: SuiAddress,
    ) -> SuiResult<Vec<(TxSequenceNumber, TransactionDigest)>> {
        Self::get_transactions_by_object(&self.transactions_from_addr, addr)
    }

    pub fn get_transactions_by_move_function(
        &self,
        package: ObjectID,
        module: Option<String>,
        function: Option<String>,
    ) -> SuiResult<Vec<(TxSequenceNumber, TransactionDigest)>> {
        Ok(self
            .transactions_by_move_function
            .iter()
            .skip_to(&(
                package,
                module.clone().unwrap_or_default(),
                function.clone().unwrap_or_default(),
                TxSequenceNumber::MIN,
            ))?
            .take_while(|((id, m, f, _), _)| {
                *id == package
                    && module.as_ref().map(|x| x == m).unwrap_or(true)
                    && function.as_ref().map(|x| x == f).unwrap_or(true)
            })
            .map(|((_, _, _, seq), digest)| (seq, digest))
            .collect())
    }

    pub fn get_transactions_to_addr(
        &self,
        addr: SuiAddress,
    ) -> SuiResult<Vec<(TxSequenceNumber, TransactionDigest)>> {
        Self::get_transactions_by_object(&self.transactions_to_addr, addr)
    }
}
