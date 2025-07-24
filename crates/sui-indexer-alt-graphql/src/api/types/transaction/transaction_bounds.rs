// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{dataloader::DataLoader, Context};
use std::ops::RangeInclusive;
use std::sync::Arc;

use sui_indexer_alt_reader::{
    cp_sequence_numbers::CpSequenceNumberKey, kv_loader::KvLoader, pg_reader::PgReader,
};

use crate::error::RpcError;
use anyhow::Context as _;

/// Bounds on transaction sequence number, imposed by filters. The outermost bounds are determined
/// by the checkpoint filters. These get translated into bounds in terms of transaction sequence numbers:
pub(crate) struct TransactionBounds {
    /// The inclusive lower bound tx_sequence_number derived from checkpoint bounds.
    tx_lo: u64,
    /// The exclusive upper bound tx_sequence_number derived from checkpoint bounds.
    tx_hi: u64,
}

impl TransactionBounds {
    /// Constructs TransactionBounds using checkpoint boundaries to map transaction sequence numbers:
    ///  - Queries the cp_sequence_numbers table to find the first transaction (tx_lo) in the lower checkpoint
    ///  - Queries the KV store for the upper checkpoint to determine the exclusive upper bound using the `network_total_transactions` field
    ///
    pub(crate) async fn fetch_transaction_bounds(
        ctx: &Context<'_>,
        cp_bounds: RangeInclusive<u64>,
    ) -> Result<TransactionBounds, RpcError> {
        let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;

        // Load the lower checkpoint sequence number data
        let cp_sequence_lo: Option<_> = pg_loader
            .load_one(CpSequenceNumberKey(*cp_bounds.start()))
            .await
            .context("Failed to query checkpoint bounds")?;

        let tx_lo = match cp_sequence_lo {
            Some(stored_cp) => stored_cp.tx_lo as u64,
            None => return Err(anyhow::anyhow!("No valid lower checkpoint bound found").into()),
        };

        let kv_loader: &KvLoader = ctx.data()?;
        let contents = kv_loader
            .load_one_checkpoint(*cp_bounds.end())
            .await
            .context("Failed to load checkpoint contents")?;

        // tx_hi_exclusive is the network_total_transactions of the highest checkpoint bound.
        let tx_hi = if let Some((summary, _, _)) = contents.as_ref() {
            summary.network_total_transactions as u64
        } else {
            return Err(anyhow::anyhow!("No valid upper tx upper bound found").into());
        };

        Ok(Self { tx_lo, tx_hi })
    }

    /// Get the lower tx bound
    pub(crate) fn lo(&self) -> u64 {
        self.tx_lo
    }

    /// Get the upper tx bound
    pub(crate) fn hi(&self) -> u64 {
        self.tx_hi
    }
}
