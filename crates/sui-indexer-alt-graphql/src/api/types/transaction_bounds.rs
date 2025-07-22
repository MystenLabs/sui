// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{ExpressionMethods, QueryDsl};
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::pg_reader::Connection;

use crate::api::types::checkpoint_bounds::CheckpointBounds;
use crate::error::RpcError;
use anyhow::Context as _;
use sui_indexer_alt_schema::schema::cp_sequence_numbers;

pub(crate) struct TransactionBounds {
    tx_lo: u64,
    tx_hi_exclusive: u64,
}

impl TransactionBounds {
    /// Constructs TransactionBounds using checkpoint boundaries to map transaction sequence numbers:
    ///  - Queries the cp_sequence_numbers table to find the first transaction (tx_lo) in the lower checkpoint
    ///  - Queries the KV store for the upper checkpoint to determine the exclusive upper bound using the `network_total_transactions` field
    ///
    pub(crate) async fn fetch_transaction_bounds(
        conn: &mut Connection<'_>,
        kv_loader: &KvLoader,
        checkpoint_bounds: CheckpointBounds,
    ) -> Result<TransactionBounds, RpcError> {
        use cp_sequence_numbers::dsl as cp;

        let cp_tx_lo_query = cp::cp_sequence_numbers
            .select(cp::tx_lo)
            .filter(cp::cp_sequence_number.eq(checkpoint_bounds.lower() as i64))
            .limit(1);

        let tx_lo_record: Vec<i64> = conn
            .results(cp_tx_lo_query)
            .await
            .context("Failed to query checkpoint sequence numbers")?;

        let tx_lo = match tx_lo_record.first().copied() {
            Some(val) => val,
            None => return Err(anyhow::anyhow!("No valid lower checkpoint bound found").into()),
        };

        let contents = kv_loader
            .load_one_checkpoint(checkpoint_bounds.upper())
            .await
            .context("Failed to load checkpoint contents")?;

        // tx_hi_exclusive is the network_total_transactions of the highest checkpoint bound.
        let tx_hi_exclusive = if let Some((summary, _, _)) = contents.as_ref() {
            summary.network_total_transactions as i64
        } else {
            return Err(anyhow::anyhow!("No valid upper tx upper bound found").into());
        };

        Ok(Self {
            tx_lo: tx_lo as u64,
            tx_hi_exclusive: tx_hi_exclusive as u64,
        })
    }
    /// Get the lower tx bound
    pub(crate) fn lower(&self) -> u64 {
        self.tx_lo
    }

    /// Get the upper tx bound
    pub(crate) fn upper_exclusive(&self) -> u64 {
        self.tx_hi_exclusive
    }
}
