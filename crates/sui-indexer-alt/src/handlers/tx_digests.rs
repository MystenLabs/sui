// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{db, models::transactions::StoredTxDigest, schema::tx_digests};

use super::Handler;

pub struct TxDigests;

#[async_trait::async_trait]
impl Handler for TxDigests {
    const NAME: &'static str = "tx_digests";

    const BATCH_SIZE: usize = 100;
    const CHUNK_SIZE: usize = 1000;
    const MAX_PENDING_SIZE: usize = 10000;

    type Value = StoredTxDigest;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let first_tx = checkpoint_summary.network_total_transactions as usize - transactions.len();

        Ok(transactions
            .iter()
            .enumerate()
            .map(|(i, tx)| StoredTxDigest {
                tx_sequence_number: (first_tx + i) as i64,
                tx_digest: tx.transaction.digest().inner().to_vec(),
            })
            .collect())
    }

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(tx_digests::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
