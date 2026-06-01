// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that feeds the
//! [`schema::balance`](crate::schema::balance) CF.
//!
//! Mirrors `index_transactions` in `sui-core::rpc_index`: for
//! every transaction in the checkpoint, call
//! [`sui_types::balance_change::derive_detailed_balance_changes_2`]
//! and forward the returned `(coin_amount, address_amount)`
//! deltas straight into the CF as a single combined merge operand
//! per `(owner, coin_type)`.
//!
//! The `derive_detailed_balance_changes_2` helper already
//! consolidates input and output coin objects (for the *coin*
//! side) and parses the effects' accumulator writes (for the
//! *address* side), so the pipeline doesn't need to walk objects
//! itself.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use move_core_types::language_storage::TypeTag;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::balance_change::derive_detailed_balance_changes_2;
use sui_types::base_types::SuiAddress;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::schema::balance;
use crate::schema::balance::Key;

/// Pipeline marker for `balance`.
pub struct Balance;

#[derive(Debug)]
pub struct Delta {
    pub owner: SuiAddress,
    pub coin_type: TypeTag,
    /// Change to the coin-derived component (sum of owned
    /// `Coin<T>` deltas).
    pub coin: i128,
    /// Change to the accumulator-derived component (sum of
    /// per-tx accumulator writes against `(owner, coin_type)`).
    pub address: i128,
}

#[async_trait]
impl Processor for Balance {
    const NAME: &'static str = "balance";
    type Value = Delta;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Delta>> {
        let mut deltas = Vec::new();
        for tx in &checkpoint.transactions {
            for change in
                derive_detailed_balance_changes_2(&tx.effects, &checkpoint.object_set)
            {
                deltas.push(Delta {
                    owner: change.address,
                    coin_type: change.coin_type,
                    coin: change.coin_amount,
                    address: change.address_amount,
                });
            }
        }
        Ok(deltas)
    }
}

#[async_trait]
impl sequential::Handler for Balance {
    type Store = Store;
    /// Combine deltas observed in this checkpoint by
    /// `(owner, coin_type)` so a single combined merge operand is
    /// staged per key instead of many small ones.
    type Batch = HashMap<Key, (i128, i128)>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Delta>) {
        for d in values {
            let entry = batch
                .entry(Key {
                    owner: d.owner,
                    coin_type: d.coin_type,
                })
                .or_insert((0, 0));
            entry.0 = entry.0.saturating_add(d.coin);
            entry.1 = entry.1.saturating_add(d.address);
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().balance;
        for (key, (coin, address)) in batch {
            let (_, value) =
                balance::delta(key.owner, key.coin_type.clone(), *coin, *address);
            conn.batch.merge(cf, key, &value)?;
        }
        Ok(batch.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _ = Balance.process(&checkpoint).await.unwrap();
    }
}
