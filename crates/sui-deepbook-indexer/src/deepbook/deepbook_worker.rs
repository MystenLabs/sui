// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{deepbook::metrics::DeepbookIndexerMetrics, models::DeepPrice};
use crate::deepbook::postgres_deepbook::write;
use crate::models::{Deepbook, DeepbookType};
use crate::postgres_manager::PgPool;
use diesel::data_types::PgTimestamp;
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
        timestamp_ms: u64,
    ) -> DeepbookType {
        // info!(
        //     "Processing deepbook transaction [{}] {}: {:#?}",
        //     checkpoint,
        //     transaction.transaction.digest().base58_encode(),
        //     transaction.transaction.transaction_data(),
        // );
        let deep_price = self.process_deep_price(transaction, checkpoint, timestamp_ms);
        if let Some(deep_price) = deep_price {
            info!("Processed deep_price: {:?}", deep_price);
            return DeepbookType::DeepPrice(deep_price);
        }
        let txn_data = transaction.transaction.transaction_data();

        let digest = transaction.transaction.digest().base58_encode();
        DeepbookType::Deepbook(Deepbook {
            digest,
            sender: txn_data.sender().to_string(),
            checkpoint: checkpoint as i64,
        })
    }

    fn process_deep_price(
        &self,
        transaction: &CheckpointTransaction,
        checkpoint: u64,
        timestamp_ms: u64,
    ) -> Option<DeepPrice> {
        let digest = transaction.transaction.digest().base58_encode();
        let txn_data = transaction.transaction.transaction_data();
        let sender = txn_data.sender().to_string();
        let command = txn_data.move_calls();
        if command.len() == 1 {
            let (_, _, fun_name) = command[0];
            if fun_name.as_str() == "add_deep_price_point" {
                info!("Found create_deep_price_point function call: {:?}", txn_data);
            }
            if let Ok(input_objects) = txn_data.input_objects() {
                if input_objects.len() >= 3 {
                    let target_pool = &input_objects[0].object_id().to_string();
                    let reference_pool = &input_objects[1].object_id().to_string();
                    return Some(DeepPrice {
                        digest,
                        sender,
                        target_pool: target_pool.to_string(),
                        reference_pool: reference_pool.to_string(),
                        checkpoint: checkpoint as i64,
                        timestamp: PgTimestamp(timestamp_ms as i64),
                    });
                }
            }
        }

        return None;
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
            .collect::<Vec<DeepbookType>>();

        write(&self.pg_pool, data).tap_ok(|_| {
            // info!("Processed checkpoint [{}] successfully", checkpoint_num);
            self.metrics.data_ingestion_checkpoint.inc();
        })
    }
}
