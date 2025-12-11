// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::transaction::{Command, TransactionDataAPI, TransactionKind};
use tracing::error;

use crate::Row;
use crate::tables::TransactionRow;

pub struct TransactionProcessor;

impl Row for TransactionRow {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for TransactionProcessor {
    const NAME: &'static str = "transactions";
    const FANOUT: usize = 10;
    type Value = TransactionRow;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let transaction_positions = compute_transaction_positions(&checkpoint.contents);

        let mut entries = Vec::with_capacity(checkpoint.transactions.len());

        for executed_transaction in &checkpoint.transactions {
            let effects = &executed_transaction.effects;
            let txn_data = &executed_transaction.transaction;
            let gas_object = effects.gas_object();
            let gas_summary = effects.gas_cost_summary();
            let move_calls_vec = txn_data.move_calls();
            let packages: BTreeSet<_> = move_calls_vec
                .iter()
                .map(|(package, _, _)| package.to_canonical_string(/* with_prefix */ false))
                .collect();
            let packages = packages
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("-");
            let transaction_digest = effects.transaction_digest().base58_encode();
            let events_digest = executed_transaction
                .events
                .as_ref()
                .map(|events| events.digest().base58_encode());

            let transaction_position = *transaction_positions
                .get(effects.transaction_digest())
                .expect("Expect transaction to exist in checkpoint_contents.")
                as u64;

            let transaction_data_bcs_length = bcs::to_bytes(&txn_data).unwrap().len() as u64;
            let effects_bcs_length =
                bcs::to_bytes(&executed_transaction.effects).unwrap().len() as u64;
            let events_bcs_length = executed_transaction
                .events
                .as_ref()
                .map(|events| bcs::to_bytes(events).unwrap().len() as u64)
                .unwrap_or(0);
            let signatures_bcs_length = bcs::to_bytes(&executed_transaction.signatures)
                .unwrap()
                .len() as u64;

            let mut transfers: u64 = 0;
            let mut split_coins: u64 = 0;
            let mut merge_coins: u64 = 0;
            let mut publish: u64 = 0;
            let mut upgrade: u64 = 0;
            let mut others: u64 = 0;
            let mut move_calls_count = 0;
            let move_calls = move_calls_vec.len() as u64;

            let is_sponsored_tx = txn_data.is_sponsored_tx();
            let is_system_txn = txn_data.is_system_tx();
            if !is_system_txn {
                let kind = txn_data.kind();
                if let TransactionKind::ProgrammableTransaction(pt) = txn_data.kind() {
                    for cmd in &pt.commands {
                        match cmd {
                            Command::MoveCall(_) => move_calls_count += 1,
                            Command::TransferObjects(_, _) => transfers += 1,
                            Command::SplitCoins(_, _) => split_coins += 1,
                            Command::MergeCoins(_, _) => merge_coins += 1,
                            Command::Publish(_, _) => publish += 1,
                            Command::Upgrade(_, _, _, _) => upgrade += 1,
                            _ => others += 1,
                        }
                    }
                } else {
                    error!(
                        "Transaction kind [{kind}] is not programmable transaction and not a system transaction"
                    );
                }
                if move_calls_count != move_calls {
                    error!(
                        "Mismatch in move calls count: commands {move_calls_count} != {move_calls} calls"
                    );
                }
            }

            let transaction_json = serde_json::to_string(&txn_data)?;
            let effects_json = serde_json::to_string(&executed_transaction.effects)?;

            let row = TransactionRow {
                transaction_digest,
                checkpoint: checkpoint_seq,
                epoch,
                timestamp_ms,
                sender: txn_data.sender().to_string(),
                transaction_kind: txn_data.kind().name().to_owned(),
                is_system_txn,
                is_sponsored_tx,
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
                transfers,
                split_coins,
                merge_coins,
                publish,
                upgrade,
                others,
                move_calls,
                packages,
                gas_owner: txn_data.gas_owner().to_string(),
                gas_object_id: gas_object.0.0.to_string(),
                gas_object_sequence: gas_object.0.1.value(),
                gas_object_digest: gas_object.0.2.to_string(),
                gas_budget: txn_data.gas_budget(),
                total_gas_cost: gas_summary.net_gas_usage(),
                computation_cost: gas_summary.computation_cost,
                storage_cost: gas_summary.storage_cost,
                storage_rebate: gas_summary.storage_rebate,
                non_refundable_storage_fee: gas_summary.non_refundable_storage_fee,
                gas_price: txn_data.gas_price(),
                has_zklogin_sig: executed_transaction
                    .signatures
                    .iter()
                    .any(|sig| sig.is_zklogin()),
                has_upgraded_multisig: executed_transaction
                    .signatures
                    .iter()
                    .any(|sig| sig.is_upgraded_multisig()),
                transaction_json: Some(transaction_json),
                effects_json: Some(effects_json),
                transaction_position,
                events_digest,
                raw_transaction: "".to_string(),
                transaction_data_bcs_length,
                effects_bcs_length,
                events_bcs_length,
                signatures_bcs_length,
            };

            entries.push(row);
        }

        Ok(entries)
    }
}

fn compute_transaction_positions(
    checkpoint_contents: &CheckpointContents,
) -> HashMap<TransactionDigest, usize> {
    let mut digest_to_position: HashMap<TransactionDigest, usize> = HashMap::new();

    for (position, execution_digest) in checkpoint_contents.iter().enumerate() {
        digest_to_position.insert(execution_digest.transaction, position);
    }

    digest_to_position
}
