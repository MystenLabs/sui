// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that feeds the *address* (accumulator) side
//! of the [`schema::balance`](crate::schema::balance) CF.
//!
//! Companion to [`balance::Balance`](crate::indexer::balance::Balance),
//! which fills the *coin* side. The two pipelines run independently
//! and stage independent merge operands against the same CF —
//! the schema's field-wise merge operator keeps their contributions
//! disjoint within each row.
//!
//! Mirrors the `get_address_balance_info` flow from
//! `sui-core::rpc_index`: walk every Move object whose parent is
//! `SUI_ACCUMULATOR_ROOT_OBJECT_ID`, parse its
//! `(AccumulatorKey, AccumulatorValue)` payload, and emit a
//! delta against `(owner, coin_type)`. Inputs contribute a
//! negated delta (the prior accumulator value is going away);
//! outputs contribute a positive delta (the new value is taking
//! its place).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use sui_types::accumulator_root::AccumulatorKey;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::base_types::SuiAddress;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;
use sui_types::object::Owner;

use crate::indexer::Schema;
use crate::indexer::Store;
use crate::indexer::checkpoint_input_objects;
use crate::indexer::checkpoint_output_objects;
use crate::schema::balance;
use crate::schema::balance::Key;

/// Pipeline marker for the accumulator side of `balance`.
pub struct AddressBalance;

#[derive(Debug)]
pub struct Delta {
    pub owner: SuiAddress,
    pub coin_type: TypeTag,
    pub delta: i128,
}

#[async_trait]
impl Processor for AddressBalance {
    const NAME: &'static str = "address_balance";
    type Value = Delta;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Delta>> {
        let mut deltas = Vec::new();
        for (_, (input, _)) in checkpoint_input_objects(checkpoint)? {
            if let Some(d) = delta_for(input) {
                deltas.push(d.negated());
            }
        }
        for (_, (output, _)) in checkpoint_output_objects(checkpoint)? {
            if let Some(d) = delta_for(output) {
                deltas.push(d);
            }
        }
        Ok(deltas)
    }
}

#[async_trait]
impl sequential::Handler for AddressBalance {
    type Store = Store;
    /// Combine deltas observed in this checkpoint by
    /// `(owner, coin_type)` before staging the merge operands —
    /// one operand per distinct key per commit instead of many
    /// small ones.
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
            let (_, value) = balance::address_delta(key.owner, key.coin_type.clone(), *delta);
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

/// Extract an accumulator-balance delta from one object, mirroring
/// `sui-core::rpc_index::get_address_balance_info`.
///
/// Returns `None` for any object that isn't an accumulator entry
/// owned by `SUI_ACCUMULATOR_ROOT_OBJECT_ID`, or whose
/// `(AccumulatorKey, AccumulatorValue)` payload doesn't parse, or
/// whose balance is non-positive.
fn delta_for(obj: &Object) -> Option<Delta> {
    if !matches!(
        obj.owner(),
        Owner::ObjectOwner(parent)
            if *parent == SuiAddress::from(SUI_ACCUMULATOR_ROOT_OBJECT_ID),
    ) {
        return None;
    }

    let move_object = obj.data.try_as_move()?;

    let coin_type: StructTag =
        match move_object.type_().balance_accumulator_field_type_maybe()? {
            TypeTag::Struct(s) => *s,
            _ => return None,
        };

    let (key, value): (AccumulatorKey, AccumulatorValue) = move_object.try_into().ok()?;
    let balance = value.as_u128()? as i128;
    if balance <= 0 {
        return None;
    }

    Some(Delta {
        owner: key.owner,
        coin_type: TypeTag::Struct(Box::new(coin_type)),
        delta: balance,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _ = AddressBalance.process(&checkpoint).await.unwrap();
    }
}
