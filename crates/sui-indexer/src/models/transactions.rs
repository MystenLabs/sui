// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::transactions;
use crate::schema::transactions::dsl;
use crate::utils::log_errors_to_pg;

use chrono::NaiveDateTime;
use diesel::dsl::{count, max};
use diesel::prelude::*;
use diesel::result::Error;
use sui_json_rpc_types::{
    OwnedObjectRef, SuiObjectRef, SuiTransaction, SuiTransactionEffects, SuiTransactionResponse,
};

use crate::errors::IndexerError;
use crate::schema::transactions::transaction_digest;
use crate::PgPoolConnection;

#[derive(Clone, Debug, Queryable)]
pub struct Transaction {
    pub id: i64,
    pub transaction_digest: String,
    pub sender: String,
    pub recipients: Vec<Option<String>>,
    pub checkpoint_sequence_number: i64,
    pub transaction_time: Option<NaiveDateTime>,
    pub transaction_kinds: Vec<Option<String>>,
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

#[derive(Clone, Debug, Insertable)]
#[diesel(table_name = transactions)]
pub struct NewTransaction {
    pub transaction_digest: String,
    pub sender: String,
    pub recipients: Vec<Option<String>>,
    pub checkpoint_sequence_number: i64,
    pub transaction_time: Option<NaiveDateTime>,
    pub transaction_kinds: Vec<Option<String>>,
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
    tx_resps: Vec<SuiTransactionResponse>,
) -> Result<usize, IndexerError> {
    let new_txn_iter = tx_resps.into_iter().map(NewTransaction::try_from);

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

impl TryFrom<SuiTransactionResponse> for NewTransaction {
    type Error = IndexerError;

    fn try_from(tx_resp: SuiTransactionResponse) -> Result<Self, Self::Error> {
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

        // canonical txn digest string is Base58 encoded
        let tx_digest = tx_resp.effects.transaction_digest.base58_encode();
        let gas_budget = tx_resp.transaction.data.gas_data.budget;
        let gas_price = tx_resp.transaction.data.gas_data.price;
        let sender = tx_resp.transaction.data.sender.to_string();
        // NOTE: unwrap is safe here because indexer fetches checkpoint first and then transactions
        // based on the transaction digests in the checkpoint, thus the checkpoint sequence number
        // is always Some. This is also confirmed by the sui-core team.
        let checkpoint_seq_number = tx_resp.checkpoint.unwrap() as i64;
        let txn_kind_iter = tx_resp
            .transaction
            .data
            .transactions
            .iter()
            .map(|k| k.to_string());

        let effects = tx_resp.effects.clone();
        let recipients: Vec<String> = effects
            .mutated
            .clone()
            .into_iter()
            .map(|owned_obj_ref| owned_obj_ref.owner.to_string())
            .collect();
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
        let move_call_strs: Vec<String> = tx_resp
            .transaction
            .data
            .clone()
            .move_calls()
            .into_iter()
            .map(|move_call| {
                let package = move_call.package.to_string();
                let module = move_call.module.to_string();
                let function = move_call.function.to_string();
                format!("{}::{}::{}", package, module, function)
            })
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
        // canonical object digest is Base58 encoded
        let gas_object_digest = gas_object_ref.digest.base58_encode();

        let gas_summary = tx_resp.effects.gas_used;
        let computation_cost = gas_summary.computation_cost;
        let storage_cost = gas_summary.storage_cost;
        let storage_rebate = gas_summary.storage_rebate;

        Ok(NewTransaction {
            transaction_digest: tx_digest,
            sender,
            recipients: vec_string_to_vec_opt_string(recipients),
            checkpoint_sequence_number: checkpoint_seq_number,
            transaction_kinds: txn_kind_iter.map(Some).collect::<Vec<Option<String>>>(),
            transaction_time: timestamp,
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

impl TryInto<SuiTransactionResponse> for Transaction {
    type Error = IndexerError;

    fn try_into(self) -> Result<SuiTransactionResponse, Self::Error> {
        let txn: SuiTransaction =
            serde_json::from_str(&self.transaction_content).map_err(|err| {
                IndexerError::InsertableParsingError(format!(
                    "Failed converting transaction JSON {:?} to SuiTransaction with error: {:?}",
                    self.transaction_content, err
                ))
            })?;
        let txn_effects: SuiTransactionEffects = serde_json::from_str(&self.transaction_effects_content).map_err(|err| {
            IndexerError::InsertableParsingError(format!(
                "Failed converting transaction effect JSON {:?} to SuiTransactionEffects with error: {:?}",
                self.transaction_effects_content, err
            ))
        })?;

        Ok(SuiTransactionResponse {
            transaction: txn,
            effects: txn_effects,
            confirmed_local_execution: self.confirmed_local_execution,
            timestamp_ms: self
                .transaction_time
                .map(|time| time.timestamp_millis() as u64),
            checkpoint: Some(self.checkpoint_sequence_number as u64),
            // TODO: Indexer need to persist event properly.
            events: Default::default(),
        })
    }
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

pub fn get_total_transaction_number(
    pg_pool_conn: &mut PgPoolConnection,
) -> Result<i64, IndexerError> {
    let txn_count_result: Result<i64, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| dsl::transactions.select(count(dsl::id)).first::<i64>(conn));

    txn_count_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading total transaction number with err: {:?}",
            e
        ))
    })
}

// NOTE: PG table serial number does not always increment by 1
// based on observations, thus `get_total_transaction_number` and
// `get_latest_transaction_sequence_number` are not always equal.
pub fn get_latest_transaction_sequence_number(
    pg_pool_conn: &mut PgPoolConnection,
) -> Result<i64, IndexerError> {
    let txn_seq_num_result: Result<i64, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            dsl::transactions
                .select(max(dsl::id))
                .first::<Option<i64>>(conn)
                // postgres serial starts from 1
                .map(|seq_num_opt| seq_num_opt.unwrap_or(0))
        });

    txn_seq_num_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading latest transaction sequence number with err: {:?}",
            e
        ))
    })
}

pub fn get_transaction_by_digest(
    pg_pool_conn: &mut PgPoolConnection,
    txn_digest: String,
) -> Result<Transaction, IndexerError> {
    let txn_read_result: Result<Transaction, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
        dsl::transactions
            .filter(transaction_digest.eq(txn_digest.clone()))
            .first::<Transaction>(conn)
    });

    txn_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading transaction with digest {} and err: {:?}",
            txn_digest, e
        ))
    })
}

pub fn get_transaction_sequence_by_digest(
    pg_pool_conn: &mut PgPoolConnection,
    txn_digest: Option<String>,
    reverse: bool,
) -> Result<i64, IndexerError> {
    let Some(txn_digest) = txn_digest else {
        if reverse {
            return get_latest_transaction_sequence_number(pg_pool_conn);
        } else {
            // NOTE: Postgres serial starts from 1
            return Ok(1);
        }
    };
    let txn_read_result: Result<i64, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            dsl::transactions
                .filter(transaction_digest.eq(txn_digest.clone()))
                .select(dsl::id)
                .first::<i64>(conn)
        });

    txn_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading transaction sequence with digest {} and err: {:?}",
            txn_digest, e
        ))
    })
}

pub fn get_all_transaction_digest_page(
    pg_pool_conn: &mut PgPoolConnection,
    start_sequence: i64,
    limit: usize,
    reverse: bool,
) -> Result<Vec<String>, IndexerError> {
    let txn_read_result: Result<Vec<String>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
        if reverse {
            dsl::transactions
                .filter(dsl::id.le(start_sequence))
                .order(dsl::id.desc())
                .limit((limit + 1) as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        } else {
            dsl::transactions
                .filter(dsl::id.ge(start_sequence))
                .order(dsl::id.asc())
                .limit((limit + 1) as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        }
    });

    txn_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading all transaction digests with start_sequence {} and limit {} and err: {:?}",
            start_sequence, limit, e
        ))
    })
}

pub fn get_transaction_digest_page_by_mutated_object(
    pg_pool_conn: &mut PgPoolConnection,
    object_id: String,
    start_sequence: i64,
    limit: usize,
    reverse: bool,
) -> Result<Vec<String>, IndexerError> {
    let txn_read_result: Result<Vec<String>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
        if reverse {
            dsl::transactions
                .filter(dsl::id.le(start_sequence))
                .filter(dsl::mutated.contains(vec![Some(object_id.clone())]))
                .order(dsl::id.desc())
                .limit(limit as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        } else {
            dsl::transactions
                .filter(dsl::id.ge(start_sequence))
                .filter(dsl::mutated.contains(vec![Some(object_id.clone())]))
                .order(dsl::id.asc())
                .limit(limit as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        }
    });

    txn_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading transaction digests by mutated object id {} with start_sequence {} and limit {} and err: {:?}",
            object_id, start_sequence, limit, e
        ))
    })
}

pub fn get_transaction_digest_page_by_sender_address(
    pg_pool_conn: &mut PgPoolConnection,
    sender_address: String,
    start_sequence: i64,
    limit: usize,
    reverse: bool,
) -> Result<Vec<String>, IndexerError> {
    let txn_read_result: Result<Vec<String>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
        if reverse {
            dsl::transactions
                .filter(dsl::id.le(start_sequence))
                .filter(dsl::sender.eq(sender_address.clone()))
                .order(dsl::id.desc())
                .limit(limit as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        } else {
            dsl::transactions
                .filter(dsl::id.ge(start_sequence))
                .filter(dsl::sender.eq(sender_address.clone()))
                .order(dsl::id.asc())
                .limit(limit as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        }
    });

    txn_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading transaction digests by sender address {} with start_sequence {} and limit {} and err: {:?}",
            sender_address, start_sequence, limit, e
        ))
    })
}

pub fn get_transaction_digest_page_by_recipient_address(
    pg_pool_conn: &mut PgPoolConnection,
    recipient_address: String,
    start_sequence: i64,
    limit: usize,
    reverse: bool,
) -> Result<Vec<String>, IndexerError> {
    let txn_read_result: Result<Vec<String>, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
        if reverse {
            dsl::transactions
                .filter(dsl::id.le(start_sequence))
                .filter(dsl::recipients.contains(vec![Some(recipient_address.clone())]))
                .order(dsl::id.desc())
                .limit(limit as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        } else {
            dsl::transactions
                .filter(dsl::id.ge(start_sequence))
                .filter(dsl::recipients.contains(vec![Some(recipient_address.clone())]))
                .order(dsl::id.asc())
                .limit(limit as i64)
                .select(transaction_digest)
                .load::<String>(conn)
        }
    });

    txn_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading transaction digests by recipient address {} with start_sequence {} and limit {} and err: {:?}",
            recipient_address, start_sequence, limit, e
        ))
    })
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
            dsl::transactions
                .filter(dsl::id.gt(last_processed_id))
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

pub fn read_latest_processed_checkpoint(
    pg_pool_conn: &mut PgPoolConnection,
) -> Result<i64, IndexerError> {
    let latest_processed_checkpoint: Result<i64, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| {
            dsl::transactions
                .select(max(dsl::checkpoint_sequence_number))
                .first::<Option<i64>>(conn)
                // -1 means no checkpoints in the DB
                .map(|o| o.unwrap_or(-1))
        });

    latest_processed_checkpoint.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading latest processed checkpoint from transaction table with err: {:?}",
            e
        ))
    })
}
