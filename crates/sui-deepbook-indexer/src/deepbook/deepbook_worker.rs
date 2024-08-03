// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::deepbook::metrics::DeepbookIndexerMetrics;
use crate::deepbook::postgres_deepbook::write;
use crate::models::Deepbook;
use crate::postgres_manager::PgPool;
use anyhow::Result;
use async_trait::async_trait;
use sui_data_ingestion_core::Worker;
use sui_types::{
    full_checkpoint_content::{CheckpointData, CheckpointTransaction},
    transaction::{TransactionDataAPI, TransactionKind},
};
use tap::tap::TapFallible;
use tracing::info;

pub struct DeepbookWorker {
    object_types: Vec<String>,
    pg_pool: PgPool,
    metrics: DeepbookIndexerMetrics,
}

impl DeepbookWorker {
    pub fn new(
        object_types: Vec<String>,
        pg_pool: PgPool,
        metrics: DeepbookIndexerMetrics,
    ) -> Self {
        Self {
            object_types,
            pg_pool,
            metrics,
        }
    }

    fn is_deepbook_transaction(&self, tx: &CheckpointTransaction) -> bool {
        let txn_data = tx.transaction.transaction_data();
        if let TransactionKind::ProgrammableTransaction(_) = txn_data.kind() {
            return tx.input_objects.iter().any(|obj| {
                obj.type_()
                    .and_then(|t| {
                        t.other().and_then(|st_tag| {
                            let tag = st_tag.to_string();
                            for obj_type in self.object_types.iter() {
                                if tag.contains(obj_type) {
                                    info!("Found deepbook type: {:?}", st_tag);
                                    return Some(true);
                                }
                            }
                            None
                        })
                    })
                    .is_some()
            });
        };
        false
    }

    // Process a transaction that has been identified as a bridge transaction.
    fn process_transaction(
        &self,
        transaction: &CheckpointTransaction,
        checkpoint: u64,
        _timestamp_ms: u64,
    ) -> Deepbook {
        info!(
            "Processing deepbook transaction [{}] {}: {:?}",
            checkpoint,
            transaction.transaction.digest().base58_encode(),
            transaction.transaction.transaction_data(),
        );
        let txn_data = transaction.transaction.transaction_data();
        let digest = transaction.transaction.digest().base58_encode();
        Deepbook {
            digest,
            sender: txn_data.sender().to_string(),
            checkpoint: checkpoint as i64,
        }
    }
}

#[async_trait]
impl Worker for DeepbookWorker {
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> Result<()> {
        let checkpoint_num = checkpoint.checkpoint_summary.sequence_number;
        let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

        let data = checkpoint
            .transactions
            .iter()
            .filter(|txn| self.is_deepbook_transaction(txn))
            .map(|txn| self.process_transaction(txn, checkpoint_num, timestamp_ms))
            .collect::<Vec<Deepbook>>();

        write(&self.pg_pool, data).tap_ok(|_| {
            info!("Processed checkpoint [{}] successfully", checkpoint_num);
            self.metrics.data_ingestion_checkpoint.inc();
        })
    }
}
