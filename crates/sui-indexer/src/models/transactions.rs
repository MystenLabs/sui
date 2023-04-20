// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use sui_json_rpc_types::{
    OwnedObjectRef, SuiObjectRef, SuiTransactionBlockDataAPI, SuiTransactionBlockEffectsAPI,
};

use crate::errors::IndexerError;
use crate::schema::transactions;
use crate::types::TemporaryTransactionBlockResponseStore;

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = transactions)]
pub struct Transaction {
    #[diesel(deserialize_as = i64)]
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub sender: String,
    pub recipients: Vec<Option<String>>,
    pub checkpoint_sequence_number: Option<i64>,
    pub timestamp_ms: Option<i64>,
    pub transaction_kind: String,
    pub transaction_count: i64,
    pub created: Vec<Option<String>>,
    pub mutated: Vec<Option<String>>,
    pub deleted: Vec<Option<String>>,
    pub unwrapped: Vec<Option<String>>,
    pub wrapped: Vec<Option<String>>,
    pub move_calls: Vec<Option<String>>,
    pub gas_object_id: String,
    pub gas_object_sequence: i64,
    pub gas_object_digest: String,
    pub gas_budget: i64,
    pub total_gas_cost: i64,
    pub computation_cost: i64,
    pub storage_cost: i64,
    pub storage_rebate: i64,
    pub non_refundable_storage_fee: i64,
    pub gas_price: i64,
    // BCS bytes of SenderSignedData
    pub raw_transaction: Vec<u8>,
    pub transaction_content: String,
    pub transaction_effects_content: String,
    pub confirmed_local_execution: Option<bool>,
}

impl TryFrom<TemporaryTransactionBlockResponseStore> for Transaction {
    type Error = IndexerError;

    fn try_from(tx_resp: TemporaryTransactionBlockResponseStore) -> Result<Self, Self::Error> {
        let TemporaryTransactionBlockResponseStore {
            digest,
            transaction,
            raw_transaction,
            effects,
            events: _,
            object_changes: _,
            balance_changes: _,
            timestamp_ms,
            confirmed_local_execution,
            checkpoint,
        } = tx_resp;

        let tx_json = serde_json::to_string(&transaction).map_err(|err| {
            IndexerError::InsertableParsingError(format!(
                "Failed converting transaction block {:?} to JSON with error: {:?}",
                transaction, err
            ))
        })?;
        let tx_effect_json = serde_json::to_string(&effects).map_err(|err| {
            IndexerError::InsertableParsingError(format!(
                "Failed converting transaction block effects {:?} to JSON with error: {:?}",
                effects.clone(),
                err
            ))
        })?;
        let recipients: Vec<String> = effects
            .mutated()
            .iter()
            .cloned()
            .chain(effects.created().iter().cloned())
            .chain(effects.unwrapped().iter().cloned())
            .map(|owned_obj_ref| owned_obj_ref.owner.to_string())
            .collect();
        let created: Vec<String> = effects
            .created()
            .iter()
            .map(owned_obj_ref_to_obj_id)
            .collect();
        let mutated: Vec<String> = effects
            .mutated()
            .iter()
            .map(owned_obj_ref_to_obj_id)
            .collect();
        let unwrapped: Vec<String> = effects
            .unwrapped()
            .iter()
            .map(owned_obj_ref_to_obj_id)
            .collect();
        let deleted: Vec<String> = effects.deleted().iter().map(obj_ref_to_obj_id).collect();
        let wrapped: Vec<String> = effects.wrapped().iter().map(obj_ref_to_obj_id).collect();
        let move_call_strs: Vec<String> = transaction
            .data
            .move_calls()
            .into_iter()
            .map(|move_call| {
                let package = move_call.package.to_string();
                let module = move_call.module.to_string();
                let function = move_call.function.to_string();
                format!("{}::{}::{}", package, module, function)
            })
            .collect();

        let gas_summary = effects.gas_cost_summary();
        let computation_cost = gas_summary.computation_cost;
        let storage_cost = gas_summary.storage_cost;
        let storage_rebate = gas_summary.storage_rebate;
        let non_refundable_storage_fee = gas_summary.non_refundable_storage_fee;
        Ok(Transaction {
            id: None,
            transaction_digest: digest.base58_encode(),
            sender: transaction.data.sender().to_string(),
            recipients: vec_string_to_vec_opt(recipients),
            checkpoint_sequence_number: checkpoint.map(|seq| seq as i64),
            transaction_kind: transaction.data.transaction().name().to_string(),
            transaction_count: transaction.data.transaction().transaction_count() as i64,
            timestamp_ms: timestamp_ms.map(|ts| ts as i64),
            created: vec_string_to_vec_opt(created),
            mutated: vec_string_to_vec_opt(mutated),
            unwrapped: vec_string_to_vec_opt(unwrapped),
            deleted: vec_string_to_vec_opt(deleted),
            wrapped: vec_string_to_vec_opt(wrapped),
            move_calls: vec_string_to_vec_opt(move_call_strs),
            gas_object_id: effects.gas_object().reference.object_id.to_string(),
            gas_object_sequence: effects.gas_object().reference.version.value() as i64,
            gas_object_digest: effects.gas_object().reference.digest.base58_encode(),
            // NOTE: cast u64 to i64 here is safe because
            // max value of i64 is 9223372036854775807 MISTs, which is 9223372036.85 SUI, which is way bigger than budget or cost constant already.
            gas_budget: transaction.data.gas_data().budget as i64,
            gas_price: transaction.data.gas_data().price as i64,
            total_gas_cost: (computation_cost + storage_cost) as i64 - (storage_rebate as i64),
            computation_cost: computation_cost as i64,
            storage_cost: storage_cost as i64,
            storage_rebate: storage_rebate as i64,
            non_refundable_storage_fee: non_refundable_storage_fee as i64,
            raw_transaction,
            transaction_content: tx_json,
            transaction_effects_content: tx_effect_json,
            confirmed_local_execution,
        })
    }
}

impl Transaction {
    // MUSTFIX(gegaowp): trim data to reduce short-term storage consumption.
    pub fn trim_data(&mut self) {
        self.created.clear();
        self.mutated.clear();
        self.unwrapped.clear();
        self.wrapped.clear();
        self.move_calls.clear();
        self.recipients.clear();
        // trim BCS and JSON data from transaction
        self.raw_transaction.clear();
        self.transaction_content.clear();
        self.transaction_effects_content.clear();
    }
}

fn owned_obj_ref_to_obj_id(owned_obj_ref: &OwnedObjectRef) -> String {
    owned_obj_ref.reference.object_id.to_string()
}

fn obj_ref_to_obj_id(obj_ref: &SuiObjectRef) -> String {
    obj_ref.object_id.to_string()
}

fn vec_string_to_vec_opt(v: Vec<String>) -> Vec<Option<String>> {
    v.into_iter().map(Some).collect::<Vec<Option<String>>>()
}
