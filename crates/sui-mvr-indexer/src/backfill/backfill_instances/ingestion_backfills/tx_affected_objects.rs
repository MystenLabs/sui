// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_instances::ingestion_backfills::IngestionBackfillTrait;
use crate::database::ConnectionPool;
use crate::models::tx_indices::StoredTxAffectedObjects;
use crate::schema::tx_affected_objects;
use diesel_async::RunQueryDsl;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::full_checkpoint_content::CheckpointData;

pub struct TxAffectedObjectsBackfill;

#[async_trait::async_trait]
impl IngestionBackfillTrait for TxAffectedObjectsBackfill {
    type ProcessedType = StoredTxAffectedObjects;

    fn process_checkpoint(checkpoint: &CheckpointData) -> Vec<Self::ProcessedType> {
        let first_tx = checkpoint.checkpoint_summary.network_total_transactions as usize
            - checkpoint.transactions.len();

        checkpoint
            .transactions
            .iter()
            .enumerate()
            .flat_map(|(i, tx)| {
                tx.effects
                    .object_changes()
                    .into_iter()
                    .map(move |change| StoredTxAffectedObjects {
                        tx_sequence_number: (first_tx + i) as i64,
                        affected: change.id.to_vec(),
                        sender: tx.transaction.sender_address().to_vec(),
                    })
            })
            .collect()
    }

    async fn commit_chunk(pool: ConnectionPool, processed_data: Vec<Self::ProcessedType>) {
        let mut conn = pool.get().await.unwrap();
        diesel::insert_into(tx_affected_objects::table)
            .values(processed_data)
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .await
            .unwrap();
    }
}
