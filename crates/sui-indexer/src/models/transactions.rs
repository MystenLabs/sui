// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::transactions;
use crate::schema::transactions::dsl::{id, transactions as transactions_table};
use crate::utils::log_errors_to_pg;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::result::Error;
use sui_json_rpc_types::{OwnedObjectRef, SuiObjectRef, SuiTransactionResponse};

use crate::errors::IndexerError;
use crate::schema::transactions::transaction_digest;
use crate::PgPoolConnection;

#[derive(Clone, Debug, Queryable)]
pub struct Transaction {
    pub id: i64,
    pub transaction_digest: String,
    pub sender: String,
    pub transaction_time: Option<NaiveDateTime>,
    pub transaction_kinds: Vec<Option<String>>,
    pub created: Vec<Option<String>>,
    pub mutated: Vec<Option<String>>,
    pub deleted: Vec<Option<String>>,
    pub unwrapped: Vec<Option<String>>,
    pub wrapped: Vec<Option<String>>,
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
}

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = transactions)]
pub struct NewTransaction {
    pub transaction_digest: String,
    pub sender: String,
    pub transaction_time: Option<NaiveDateTime>,
    pub transaction_kinds: Vec<Option<String>>,
    pub created: Vec<Option<String>>,
    pub mutated: Vec<Option<String>>,
    pub deleted: Vec<Option<String>>,
    pub unwrapped: Vec<Option<String>>,
    pub wrapped: Vec<Option<String>>,
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
}

pub fn read_transactions(
    pg_pool_conn: &mut PgPoolConnection,
    last_processed_id: i64,
    limit: usize,
) -> Result<Vec<Transaction>, IndexerError> {
    let txn_read_result: Result<Vec<Transaction>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            transactions_table
                .filter(id.gt(last_processed_id))
                .limit(limit as i64)
                .load::<Transaction>(conn)
        });

    txn_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading transactions with last_processed_id {} and err: {:?}",
            last_processed_id, e
        ))
    })
}

pub fn commit_transactions(
    pg_pool_conn: &mut PgPoolConnection,
    tx_resps: Vec<SuiTransactionResponse>,
) -> Result<usize, IndexerError> {
    let new_txn_iter = tx_resps
        .into_iter()
        .map(transaction_response_to_new_transaction);

    let mut errors = vec![];
    let new_txns: Vec<NewTransaction> = new_txn_iter
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

pub fn transaction_response_to_new_transaction(
    tx_resp: SuiTransactionResponse,
) -> Result<NewTransaction, IndexerError> {
    let cer = tx_resp.certificate;
    let txn_json = serde_json::to_string(&cer).map_err(|err| {
        IndexerError::InsertableParsingError(format!(
            "Failed converting transaction {:?} to JSON with error: {:?}",
            cer.clone(),
            err
        ))
    })?;
    // canonical txn digest string is Base58 encoded
    let tx_digest = cer.transaction_digest.base58_encode();
    let gas_budget = cer.data.gas_data.gas_budget;
    let gas_price = cer.data.gas_data.gas_price;
    let sender = cer.data.sender.to_string();
    let txn_kind_iter = cer.data.transactions.iter().map(|k| k.to_string());

    let effects = tx_resp.effects.clone();
    let created: Vec<String> = effects
        .created
        .into_iter()
        .map(owned_obj_ref_to_obj_id_string)
        .collect();
    let mutated: Vec<String> = effects
        .mutated
        .into_iter()
        .map(owned_obj_ref_to_obj_id_string)
        .collect();
    let unwrapped: Vec<String> = effects
        .unwrapped
        .into_iter()
        .map(owned_obj_ref_to_obj_id_string)
        .collect();
    let deleted: Vec<String> = effects
        .deleted
        .into_iter()
        .map(obj_ref_to_obj_id_string)
        .collect();
    let wrapped: Vec<String> = effects
        .wrapped
        .into_iter()
        .map(obj_ref_to_obj_id_string)
        .collect();

    let timestamp_opt_res = tx_resp.timestamp_ms.map(|time_milis| {
        let naive_time = NaiveDateTime::from_timestamp_millis(time_milis as i64);
        naive_time.ok_or_else(|| {
            IndexerError::InsertableParsingError(format!(
                "Failed parsing timestamp in millis {:?} to NaiveDateTime",
                time_milis
            ))
        })
    });
    let timestamp = match timestamp_opt_res {
        Some(Err(e)) => return Err(e),
        Some(Ok(n)) => Some(n),
        None => None,
    };

    let gas_object_ref = tx_resp.effects.gas_object.reference.clone();
    let gas_object_id = gas_object_ref.object_id.to_string();
    let gas_object_seq = gas_object_ref.version;
    // canonical object digest is Base64 encoded
    let gas_object_digest = gas_object_ref.digest.base64_encode();

    let gas_summary = tx_resp.effects.gas_used;
    let computation_cost = gas_summary.computation_cost;
    let storage_cost = gas_summary.storage_cost;
    let storage_rebate = gas_summary.storage_rebate;

    Ok(NewTransaction {
        transaction_digest: tx_digest,
        sender,
        transaction_kinds: txn_kind_iter.map(Some).collect::<Vec<Option<String>>>(),
        transaction_time: timestamp,
        created: vec_string_to_vec_opt_string(created),
        mutated: vec_string_to_vec_opt_string(mutated),
        unwrapped: vec_string_to_vec_opt_string(unwrapped),
        deleted: vec_string_to_vec_opt_string(deleted),
        wrapped: vec_string_to_vec_opt_string(wrapped),
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
    })
}

fn owned_obj_ref_to_obj_id_string(owned_obj_ref: OwnedObjectRef) -> String {
    owned_obj_ref.reference.object_id.to_string()
}

fn obj_ref_to_obj_id_string(obj_ref: SuiObjectRef) -> String {
    obj_ref.object_id.to_string()
}

fn vec_string_to_vec_opt_string(v: Vec<String>) -> Vec<Option<String>> {
    v.into_iter().map(Some).collect::<Vec<Option<String>>>()
}
