// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::postgres_writer::{get_connection_pool, write, PgPool};
use crate::{TokenTransfer, TokenTransferStatus};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeSet;
use sui_data_ingestion_core::Worker;
use sui_types::{
    base_types::ObjectID,
    full_checkpoint_content::{CheckpointData, CheckpointTransaction},
    transaction::{TransactionDataAPI, TransactionKind},
    SUI_BRIDGE_OBJECT_ID,
};
use tracing::info;

pub struct BridgeWorker {
    bridge_object_ids: BTreeSet<ObjectID>,
    pg_pool: PgPool,
}

impl BridgeWorker {
    pub fn new(bridge_object_ids: Vec<ObjectID>, db_url: String) -> Self {
        let mut bridge_object_ids = bridge_object_ids.into_iter().collect::<BTreeSet<_>>();
        bridge_object_ids.insert(SUI_BRIDGE_OBJECT_ID);
        let pg_pool = get_connection_pool(db_url);
        Self {
            bridge_object_ids,
            pg_pool,
        }
    }

    // Return true if the transaction relates to the bridge and is of interest.
    fn is_bridge_transaction(&self, tx: &CheckpointTransaction) -> bool {
        // TODO: right now this returns true for programmable transactions that
        //       have the bridge object as input. We can extend later to cover other cases
        let txn_data = tx.transaction.transaction_data();
        if let TransactionKind::ProgrammableTransaction(_pt) = txn_data.kind() {
            return tx
                .input_objects
                .iter()
                .any(|obj| self.bridge_object_ids.contains(&obj.id()));
        };
        false
    }

    // Process a transaction that has been identified as a bridge transaction.
    fn process_transaction(&self, _tx: &CheckpointTransaction, _epoch: u64, _checkpoint: u64) {
        // todo create TokenTransfer from checkpoint data
        println!("SUI: Processing transaction");
        let transfer = TokenTransfer {
            chain_id: 0,
            nonce: 0,
            block_height: 0,
            timestamp_ms: Default::default(),
            txn_hash: vec![],
            status: TokenTransferStatus::Deposited,
            gas_usage: 0,
            data: None,
        };
        write(&self.pg_pool, transfer);
    }
}

#[async_trait]
impl Worker for BridgeWorker {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()> {
        info!(
            "Processing checkpoint [{}] {}: {}",
            checkpoint.checkpoint_summary.epoch,
            checkpoint.checkpoint_summary.sequence_number,
            checkpoint.transactions.len(),
        );
        let epoch = checkpoint.checkpoint_summary.epoch;
        let checkpoint_num = checkpoint.checkpoint_summary.sequence_number;
        checkpoint
            .transactions
            .iter()
            .filter(|txn| self.is_bridge_transaction(txn))
            .for_each(|txn| self.process_transaction(txn, epoch, checkpoint_num));
        Ok(())
    }
}
