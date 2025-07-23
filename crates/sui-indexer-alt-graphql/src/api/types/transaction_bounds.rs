// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{dataloader::DataLoader, Context};
use std::sync::Arc;
use sui_indexer_alt_reader::cp_sequence_numbers::CpSequenceNumberKey;
use sui_indexer_alt_reader::kv_loader::KvLoader;
use sui_indexer_alt_reader::pg_reader::PgReader;

use crate::api::types::checkpoint_bounds::CheckpointBounds;
use crate::error::RpcError;
use anyhow::Context as _;

/// Bounds on transaction sequence number, imposed by filters. The outermost bounds are determined
/// by the checkpoint filters. These get translated into bounds in terms of transaction sequence numbers:
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
        ctx: &Context<'_>,
        checkpoint_bounds: CheckpointBounds,
    ) -> Result<TransactionBounds, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        // Load the lower checkpoint sequence number data
        let cp_sequence_lo = pg_loader
            .load_one(CpSequenceNumberKey(checkpoint_bounds.lower()))
            .await
            .context("Failed to query checkpoint bounds")?;

        let tx_lo = match cp_sequence_lo {
            Some(stored_cp) => stored_cp.tx_lo as u64,
            None => return Err(anyhow::anyhow!("No valid lower checkpoint bound found").into()),
        };

        let kv_loader: &KvLoader = ctx.data()?;
        let contents = kv_loader
            .load_one_checkpoint(checkpoint_bounds.upper())
            .await
            .context("Failed to load checkpoint contents")?;

        // tx_hi_exclusive is the network_total_transactions of the highest checkpoint bound.
        let tx_hi_exclusive = if let Some((summary, _, _)) = contents.as_ref() {
            summary.network_total_transactions as u64
        } else {
            return Err(anyhow::anyhow!("No valid upper tx upper bound found").into());
        };

        Ok(Self {
            tx_lo,
            tx_hi_exclusive,
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
