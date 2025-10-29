// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sui_indexer_alt_framework::{
    pipeline::{Processor, sequential},
    types::{
        TypeTag,
        base_types::SuiAddress,
        coin::Coin,
        full_checkpoint_content::Checkpoint,
        object::{Object, Owner},
    },
};

use crate::{
    Schema,
    restore::Restore,
    schema::balances::Key,
    store::{Connection, Store},
};

use super::{checkpoint_input_objects, checkpoint_output_objects};

pub(crate) struct Balances;

#[derive(Serialize, Deserialize)]
pub struct Delta {
    owner: SuiAddress,
    type_: TypeTag,
    delta: i128,
}

impl Delta {
    fn negated(self) -> Self {
        Self {
            owner: self.owner,
            type_: self.type_,
            delta: -self.delta,
        }
    }
}

#[async_trait]
impl Processor for Balances {
    const NAME: &'static str = "balances";
    type Value = Delta;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Delta>> {
        let mut deltas = vec![];

        for (_, (i, _)) in checkpoint_input_objects(checkpoint)? {
            if let Some(d) = delta(i)? {
                deltas.push(d.negated())
            };
        }

        for (_, (o, _)) in checkpoint_output_objects(checkpoint)? {
            if let Some(d) = delta(o)? {
                deltas.push(d)
            };
        }

        Ok(deltas)
    }
}

impl Restore<Schema> for Balances {
    fn restore(
        schema: &Schema,
        object: &Object,
        batch: &mut rocksdb::WriteBatch,
    ) -> anyhow::Result<()> {
        if let Some(d) = delta(object)? {
            schema.balances.merge(
                &Key {
                    owner: d.owner,
                    type_: d.type_,
                },
                d.delta,
                batch,
            )?;
        }

        Ok(())
    }
}

#[async_trait]
impl sequential::Handler for Balances {
    type Store = Store<Schema>;
    type Batch = BTreeMap<Key, i128>;

    /// Submit a write for every checkpoint, for snapshotting purposes.
    const MAX_BATCH_CHECKPOINTS: usize = 1;

    /// Values are not batched between checkpoints, but we can simplify the output for a single
    /// checkpoint by combining deltas for the same owner and type.
    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Delta>) {
        for value in values {
            batch
                .entry(Key {
                    owner: value.owner,
                    type_: value.type_,
                })
                .and_modify(|v| *v += value.delta)
                .or_insert(value.delta);
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let balances = &conn.store.schema().balances;
        for (key, delta) in batch {
            balances.merge(key, delta, &mut conn.batch)?;
        }

        Ok(batch.len())
    }
}

fn delta(obj: &Object) -> anyhow::Result<Option<Delta>> {
    // Balances are only tracked for address owners. Balances are combined for coins that
    // are address-owned and consensus address-owned for the same address.
    let &owner = match obj.owner() {
        Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => owner,
        Owner::ObjectOwner(_) | Owner::Shared { .. } | Owner::Immutable => return Ok(None),
    };

    // Only track coins.
    let Some((type_, balance)) = Coin::extract_balance_if_coin(obj)? else {
        return Ok(None);
    };

    Ok(Some(Delta {
        owner,
        type_,
        delta: balance as i128,
    }))
}
