// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use anyhow::Result;
use fastcrypto::encoding::{Base64, Encoding};

use sui_indexer::framework::Handler;
use sui_rest_api::{CheckpointData, CheckpointTransaction};
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::TransactionDataAPI;

use crate::handlers::AnalyticsHandler;
use crate::tables::TransactionEntry;
use crate::FileType;

pub struct TransactionHandler {
    transactions: Vec<TransactionEntry>,
}

#[async_trait::async_trait]
impl Handler for TransactionHandler {
    fn name(&self) -> &str {
        "transaction"
    }
    async fn process_checkpoint(&mut self, checkpoint_data: &CheckpointData) -> Result<()> {
        let CheckpointData {
            checkpoint_summary,
            transactions: checkpoint_transactions,
            ..
        } = checkpoint_data;
        for checkpoint_transaction in checkpoint_transactions {
            self.process_transaction(
                checkpoint_summary.epoch,
                checkpoint_summary.sequence_number,
                checkpoint_summary.timestamp_ms,
                checkpoint_transaction,
                &checkpoint_transaction.effects,
            );
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl AnalyticsHandler<TransactionEntry> for TransactionHandler {
    fn read(&mut self) -> Result<Vec<TransactionEntry>> {
        let cloned = self.transactions.clone();
        self.transactions.clear();
        Ok(cloned)
    }

    fn file_type(&self) -> Result<FileType> {
        Ok(FileType::Transaction)
    }
}

impl TransactionHandler {
    pub fn new() -> Self {
        TransactionHandler {
            transactions: vec![],
        }
    }
    fn process_transaction(
        &mut self,
        epoch: u64,
        checkpoint: u64,
        timestamp_ms: u64,
        checkpoint_transaction: &CheckpointTransaction,
        effects: &TransactionEffects,
    ) {
        let transaction = &checkpoint_transaction.transaction;
        let txn_data = transaction.transaction_data();
        let gas_object = effects.gas_object();
        let gas_summary = effects.gas_cost_summary();
        let move_calls = txn_data.move_calls();
        let packages: BTreeSet<_> = move_calls
            .iter()
            .map(|(package, _, _)| package.to_canonical_string())
            .collect();
        let packages = packages
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let transaction_digest = transaction.digest().base58_encode();

        let entry = TransactionEntry {
            transaction_digest,
            checkpoint,
            epoch,
            timestamp_ms,

            sender: txn_data.sender().to_string(),
            transaction_kind: txn_data.kind().name().to_owned(),
            transaction_count: txn_data.kind().num_commands() as u64,
            execution_success: effects.status().is_ok(),
            input: txn_data
                .input_objects()
                .expect("Input objects must be valid")
                .len() as u64,
            shared_input: txn_data.shared_input_objects().len() as u64,
            gas_coins: txn_data.gas().len() as u64,
            created: effects.created().len() as u64,
            mutated: (effects.mutated().len() + effects.unwrapped().len()) as u64,
            deleted: (effects.deleted().len()
                + effects.unwrapped_then_deleted().len()
                + effects.wrapped().len()) as u64,
            move_calls: move_calls.len() as u64,
            packages,
            gas_object_id: gas_object.0 .0.to_string(),
            gas_object_sequence: gas_object.0 .1.value(),
            gas_object_digest: gas_object.0 .2.to_string(),
            gas_budget: txn_data.gas_budget(),
            total_gas_cost: gas_summary.net_gas_usage(),
            computation_cost: gas_summary.computation_cost,
            storage_cost: gas_summary.storage_cost,
            storage_rebate: gas_summary.storage_rebate,
            non_refundable_storage_fee: gas_summary.non_refundable_storage_fee,

            gas_price: txn_data.gas_price(),

            raw_transaction: Base64::encode(bcs::to_bytes(&txn_data).unwrap()),
        };
        self.transactions.push(entry);
    }
}
