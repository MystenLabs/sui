// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::transactions;
use crate::utils::log_errors_to_pg;

use diesel::prelude::*;
use diesel::result::Error;
use sui_json_rpc_types::{
    OwnedObjectRef, SuiObjectRef, SuiTransaction, SuiTransactionDataAPI, SuiTransactionEffects,
    SuiTransactionEffectsAPI,
};

use crate::errors::IndexerError;
use crate::schema::transactions::transaction_digest;
use crate::types::SuiTransactionFullResponse;
use crate::PgPoolConnection;

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = transactions)]
pub struct Transaction {
    #[diesel(deserialize_as = i64)]
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub sender: String,
    pub recipients: Vec<Option<String>>,
    pub checkpoint_sequence_number: i64,
    pub timestamp_ms: i64,
    pub transaction_kind: String,
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
    pub gas_price: i64,
    pub transaction_content: String,
    pub transaction_effects_content: String,
    pub confirmed_local_execution: Option<bool>,
}

pub fn commit_transactions(
    pg_pool_conn: &mut PgPoolConnection,
    tx_resps: Vec<SuiTransactionFullResponse>,
) -> Result<usize, IndexerError> {
    let new_txn_iter = tx_resps.into_iter().map(|tx| tx.try_into());

    let mut errors = vec![];
    let new_txns: Vec<Transaction> = new_txn_iter
        .filter_map(|r| r.map_err(|e| errors.push(e)).ok())
        .collect();
    log_errors_to_pg(pg_pool_conn, errors);

    let txn_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
        diesel::insert_into(transactions::table)
            .values(&new_txns)
            .on_conflict(transaction_digest)
            .do_nothing()
            .execute(conn)
    });

    txn_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing transactions to PostgresDB with transactions {:?} and error: {:?}",
            new_txns, e
        ))
    })
}

impl TryFrom<SuiTransactionFullResponse> for Transaction {
    type Error = IndexerError;

    fn try_from(tx_resp: SuiTransactionFullResponse) -> Result<Self, Self::Error> {
        let txn_json = serde_json::to_string(&tx_resp.transaction).map_err(|err| {
            IndexerError::InsertableParsingError(format!(
                "Failed converting transaction {:?} to JSON with error: {:?}",
                tx_resp.transaction, err
            ))
        })?;
        let txn_effect_json = serde_json::to_string(&tx_resp.effects).map_err(|err| {
            IndexerError::InsertableParsingError(format!(
                "Failed converting transaction effects {:?} to JSON with error: {:?}",
                tx_resp.effects.clone(),
                err
            ))
        })?;

        let effects = tx_resp.effects;
        let transaction_data = tx_resp.transaction.data;
        // canonical txn digest string is Base58 encoded
        let tx_digest = effects.transaction_digest().base58_encode();
        let gas_budget = transaction_data.gas_data().budget;
        let gas_price = transaction_data.gas_data().price;
        let sender = transaction_data.sender().to_string();
        let checkpoint_seq_number = tx_resp.checkpoint as i64;
        let tx_kind = transaction_data.transaction().to_string();

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
            .map(owned_obj_ref_to_obj_id_string)
            .collect();
        let mutated: Vec<String> = effects
            .mutated()
            .iter()
            .map(owned_obj_ref_to_obj_id_string)
            .collect();
        let unwrapped: Vec<String> = effects
            .unwrapped()
            .iter()
            .map(owned_obj_ref_to_obj_id_string)
            .collect();
        let deleted: Vec<String> = effects
            .deleted()
            .iter()
            .map(obj_ref_to_obj_id_string)
            .collect();
        let wrapped: Vec<String> = effects
            .wrapped()
            .iter()
            .map(obj_ref_to_obj_id_string)
            .collect();
        let move_call_strs: Vec<String> = transaction_data
            .move_calls()
            .into_iter()
            .map(|move_call| {
                let package = move_call.package.to_string();
                let module = move_call.module.to_string();
                let function = move_call.function.to_string();
                format!("{}::{}::{}", package, module, function)
            })
            .collect();

        let gas_object_ref = effects.gas_object().reference.clone();
        let gas_object_id = gas_object_ref.object_id.to_string();
        let gas_object_seq = gas_object_ref.version;
        // canonical object digest is Base58 encoded
        let gas_object_digest = gas_object_ref.digest.base58_encode();

        let gas_summary = effects.gas_used();
        let computation_cost = gas_summary.computation_cost;
        let storage_cost = gas_summary.storage_cost;
        let storage_rebate = gas_summary.storage_rebate;

        Ok(Transaction {
            id: None,
            transaction_digest: tx_digest,
            sender,
            recipients: vec_string_to_vec_opt_string(recipients),
            checkpoint_sequence_number: checkpoint_seq_number,
            transaction_kind: tx_kind,
            timestamp_ms: tx_resp.timestamp_ms as i64,
            created: vec_string_to_vec_opt_string(created),
            mutated: vec_string_to_vec_opt_string(mutated),
            unwrapped: vec_string_to_vec_opt_string(unwrapped),
            deleted: vec_string_to_vec_opt_string(deleted),
            wrapped: vec_string_to_vec_opt_string(wrapped),
            move_calls: vec_string_to_vec_opt_string(move_call_strs),
            gas_object_id,
            gas_object_sequence: gas_object_seq.value() as i64,
            gas_object_digest,
            // NOTE: cast u64 to i64 here is safe because
            // max value of i64 is 9223372036854775807 MISTs, which is 9223372036.85 SUI, which is way bigger than budget or cost constant already.
            gas_budget: gas_budget as i64,
            gas_price: gas_price as i64,
            total_gas_cost: (computation_cost + storage_cost) as i64 - (storage_rebate as i64),
            computation_cost: computation_cost as i64,
            storage_cost: storage_cost as i64,
            storage_rebate: storage_rebate as i64,
            transaction_content: txn_json,
            transaction_effects_content: txn_effect_json,
            confirmed_local_execution: tx_resp.confirmed_local_execution,
        })
    }
}

impl TryInto<SuiTransactionFullResponse> for Transaction {
    type Error = IndexerError;

    fn try_into(self) -> Result<SuiTransactionFullResponse, Self::Error> {
        let transaction: SuiTransaction =
            serde_json::from_str(&self.transaction_content).map_err(|err| {
                IndexerError::InsertableParsingError(format!(
                    "Failed converting transaction JSON {:?} to SuiTransaction with error: {:?}",
                    self.transaction_content, err
                ))
            })?;
        let effects: SuiTransactionEffects = serde_json::from_str(&self.transaction_effects_content).map_err(|err| {
            IndexerError::InsertableParsingError(format!(
                "Failed converting transaction effect JSON {:?} to SuiTransactionEffects with error: {:?}",
                self.transaction_effects_content, err
            ))
        })?;

        Ok(SuiTransactionFullResponse {
            digest: self.transaction_digest.parse().map_err(|e| {
                IndexerError::InsertableParsingError(format!(
                    "Failed to parse transaction digest {} : {:?}",
                    self.transaction_digest, e
                ))
            })?,
            transaction,
            effects,
            confirmed_local_execution: self.confirmed_local_execution,
            timestamp_ms: self.timestamp_ms as u64,
            checkpoint: self.checkpoint_sequence_number as u64,
            // TODO: read events, object_changes and balance_changes from db
            events: Default::default(),
            object_changes: Some(vec![]),
            balance_changes: Some(vec![]),
        })
    }
}

fn owned_obj_ref_to_obj_id_string(owned_obj_ref: &OwnedObjectRef) -> String {
    owned_obj_ref.reference.object_id.to_string()
}

fn obj_ref_to_obj_id_string(obj_ref: &SuiObjectRef) -> String {
    obj_ref.object_id.to_string()
}

fn vec_string_to_vec_opt_string(v: Vec<String>) -> Vec<Option<String>> {
    v.into_iter().map(Some).collect::<Vec<Option<String>>>()
}
