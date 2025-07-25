// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Context;
use std::ops::RangeInclusive;

use sui_indexer_alt_reader::kv_loader::KvLoader;

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
    ///  - Loads the lower and upper checkpoint from the KV store
    ///  - Calculates the tx_lo (exclusive) by using the upper checkpoint's network_total_transactions
    ///
    pub(crate) async fn fetch(
        ctx: &Context<'_>,
        cp_bounds: RangeInclusive<u64>,
    ) -> Result<TransactionBounds, RpcError> {
        let kv_loader: &KvLoader = ctx.data()?;

        // TODO: Should we make a load_many for checkpoints?
        let (cp_lo, cp_hi) = tokio::try_join!(
            kv_loader.load_one_checkpoint(*cp_bounds.start()),
            kv_loader.load_one_checkpoint(*cp_bounds.end())
        )
        .context("Failed to load checkpoint bounds.")?;

        let tx_lo = match cp_lo {
            Some((summary, contents, _)) => {
                summary.network_total_transactions - contents.inner().len() as u64
            }
            None => {
                return Err(RpcError::from(anyhow::anyhow!(
                    "No valid lower checkpoint bound found."
                )))
            }
        };

        // tx_hi_exclusive is the network_total_transactions of the highest checkpoint bound.
        let tx_hi = match cp_hi {
            Some((summary, _, _)) => summary.network_total_transactions,
            None => {
                return Err(RpcError::from(anyhow::anyhow!(
                    "No valid upper checkpoint bound found."
                )))
            }
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
