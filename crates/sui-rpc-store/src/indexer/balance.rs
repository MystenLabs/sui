// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that feeds the coin-derived component of
//! the [`schema::balance`](crate::schema::balance) CF.
//!
//! For each address-owned `Coin<T>` object the checkpoint changes,
//! the pipeline emits a signed `i128` delta against
//! `(owner, coin_type)`: negative for the input (the coin going
//! away or being modified) and positive for the output (the coin
//! arriving or being remodified). The schema's merge operator
//! accumulates the deltas field-wise so the on-disk row reflects
//! the running coin balance.
//!
//! The accumulator-balance pipeline is a separate concern: it
//! lives in [`address_balance`] and writes the `address` field of
//! the same `BalanceDelta` row. (It's not yet implemented — Sui's
//! address-accumulator object model is still in flux.)
//!
//! [`address_balance`]: <Not yet implemented>

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use move_core_types::language_storage::TypeTag;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::base_types::SuiAddress;
use sui_types::coin::Coin;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;
use sui_types::object::Owner;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::checkpoint_input_objects;
use crate::indexer::checkpoint_output_objects;
use crate::schema::balance;
use crate::schema::balance::Key;

/// Pipeline marker for `balance` (coin side).
pub struct Balance;

#[derive(Debug)]
pub struct Delta {
    pub owner: SuiAddress,
    pub coin_type: TypeTag,
    pub delta: i128,
}

#[async_trait]
impl Processor for Balance {
    const NAME: &'static str = "balance";
    type Value = Delta;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Delta>> {
        let mut deltas = Vec::new();
        for (_, (input, _)) in checkpoint_input_objects(checkpoint)? {
            if let Some(d) = delta_for(input)? {
                deltas.push(d.negated());
            }
        }
        for (_, (output, _)) in checkpoint_output_objects(checkpoint)? {
            if let Some(d) = delta_for(output)? {
                deltas.push(d);
            }
        }
        Ok(deltas)
    }
}

#[async_trait]
impl sequential::Handler for Balance {
    type Store = Store;
    /// Combine the deltas observed in this checkpoint by
    /// `(owner, coin_type)` before staging the merge operands —
    /// avoids redundant writes when a single key would otherwise
    /// receive several small operands in one commit.
    type Batch = HashMap<Key, i128>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Delta>) {
        for d in values {
            *batch
                .entry(Key {
                    owner: d.owner,
                    coin_type: d.coin_type,
                })
                .or_insert(0) += d.delta;
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().balance;
        for (key, delta) in batch {
            let (_, value) = balance::coin_delta(key.owner, key.coin_type.clone(), *delta);
            conn.batch.merge(cf, key, &value)?;
        }
        Ok(batch.len())
    }
}

impl Delta {
    fn negated(self) -> Self {
        Self {
            owner: self.owner,
            coin_type: self.coin_type,
            delta: self.delta.wrapping_neg(),
        }
    }
}

/// Extract a coin-balance delta from one Move object.
///
/// Returns `None` for non-Move objects, non-coin Move objects,
/// and objects whose owner isn't an address (shared, immutable,
/// or owned by another object). `ConsensusAddressOwner` is folded
/// into `AddressOwner` so the same address sees its balance
/// whether the coin sits on the consensus path or not.
fn delta_for(obj: &Object) -> anyhow::Result<Option<Delta>> {
    let &owner = match obj.owner() {
        Owner::AddressOwner(a) | Owner::ConsensusAddressOwner { owner: a, .. } => a,
        Owner::ObjectOwner(_) | Owner::Shared { .. } | Owner::Immutable => return Ok(None),
        Owner::Party { .. } => anyhow::bail!("Party owner WIP"),
    };

    let Some((coin_type, balance)) = Coin::extract_balance_if_coin(obj)? else {
        return Ok(None);
    };

    Ok(Some(Delta {
        owner,
        coin_type,
        delta: balance as i128,
    }))
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
