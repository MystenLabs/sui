// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use sui_json_rpc_types::{SuiTransactionBlockDataAPI, SuiTransactionBlockEffectsAPI};

use crate::errors::IndexerError;
use crate::schema::transactions;
use crate::types::TemporaryTransactionBlockResponseStore;

#[derive(Clone, Debug, Queryable, Insertable, QueryableByName)]
#[diesel(table_name = transactions)]
pub struct Transaction {
    #[diesel(deserialize_as = i64)]
    pub id: Option<i64>,
    pub transaction_digest: String,
    pub sender: String,
    pub checkpoint_sequence_number: Option<i64>,
    pub timestamp_ms: Option<i64>,
    pub transaction_kind: String,
    pub transaction_count: i64,
    pub execution_success: bool,
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

        let tx_effect_json = serde_json::to_string(&effects).map_err(|err| {
            IndexerError::InsertableParsingError(format!(
                "Failed converting transaction block effects {:?} to JSON with error: {:?}",
                effects.clone(),
                err
            ))
        })?;

        let gas_summary = effects.gas_cost_summary();
        let computation_cost = gas_summary.computation_cost;
        let storage_cost = gas_summary.storage_cost;
        let storage_rebate = gas_summary.storage_rebate;
        let non_refundable_storage_fee = gas_summary.non_refundable_storage_fee;
        Ok(Transaction {
            id: None,
            transaction_digest: digest.base58_encode(),
            sender: transaction.data.sender().to_string(),
            checkpoint_sequence_number: checkpoint.map(|seq| seq as i64),
            transaction_kind: transaction.data.transaction().name().to_string(),
            transaction_count: transaction.data.transaction().transaction_count() as i64,
            execution_success: effects.status().is_ok(),
            timestamp_ms: timestamp_ms.map(|ts| ts as i64),
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
            transaction_effects_content: tx_effect_json,
            confirmed_local_execution,
        })
    }
}
