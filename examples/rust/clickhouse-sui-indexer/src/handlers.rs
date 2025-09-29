// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use clickhouse::Row;
use serde::Serialize;
use std::sync::Arc;

use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    FieldCount,
};
use sui_indexer_alt_framework_store_traits::Store;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::store::ClickHouseStore;

/// Structure representing a transaction digest record in ClickHouse
/// Aligned with sui-indexer-alt's StoredTxDigest structure
#[derive(Row, Serialize, Clone, Debug, FieldCount)]
pub struct StoredTxDigest {
    pub tx_sequence_number: i64,
    pub tx_digest: Vec<u8>,
}

/// Handler that processes checkpoint data and extracts transaction digests
/// Named to align with sui-indexer-alt's TxDigests handler
#[derive(Clone, Default)]
pub struct TxDigests;

impl Processor for TxDigests {
    const NAME: &'static str = "tx_digests";
    type Value = StoredTxDigest;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
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
}

#[async_trait]
impl Handler for TxDigests {
    type Store = ClickHouseStore;

    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> Result<usize> {
        let row_count = values.len();
        if row_count == 0 {
            return Ok(0);
        }

        // Use ClickHouse inserter for efficient bulk inserts
        let mut inserter = conn.client.inserter("tx_digests")?;
        for tx_digest in values {
            inserter.write(tx_digest)?;
        }
        inserter.end().await?;

        Ok(row_count)
    }
}
