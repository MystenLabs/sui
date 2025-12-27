// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use anyhow::Context;
use async_trait::async_trait;
use sui_indexer_alt_framework::{
    pipeline::{Processor, sequential},
    types::{
        base_types::SuiAddress,
        dynamic_field::Field,
        full_checkpoint_content::Checkpoint,
        object::Object,
        transaction::{TransactionDataAPI, TransactionKind},
    },
};

use crate::store::{Connection, Store};
use crate::{
    restore::Restore,
    schema::{Schema, address_balances::Key},
};

pub(crate) struct AddressBalances;

#[async_trait]
impl Processor for AddressBalances {
    const NAME: &'static str = "address_balances";
    type Value = (Key, u128);

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let mut results = vec![];

        for transaction in &checkpoint.transactions {
            if !matches!(
                transaction.transaction.kind(),
                TransactionKind::ProgrammableSystemTransaction(_)
            ) {
                continue;
            }

            let mut modified = BTreeSet::new();

            for obj in transaction.output_objects(&checkpoint.object_set) {
                let Some(entry) = try_extract_balance(obj)? else {
                    continue;
                };

                modified.insert(obj.id());
                results.push(entry);
            }

            // Address balance input objects that don't appear in the output will have its balance
            // marked as 0. These will be marked for deletion on commit.
            for obj in transaction.input_objects(&checkpoint.object_set) {
                if modified.contains(&obj.id()) {
                    continue;
                }

                let Some(entry) = try_extract_balance(obj)? else {
                    continue;
                };

                results.push((entry.0, 0u128));
            }
        }

        Ok(results)
    }
}

impl Restore<Schema> for AddressBalances {
    fn restore(
        schema: &Schema,
        object: &Object,
        batch: &mut rocksdb::WriteBatch,
    ) -> anyhow::Result<()> {
        if let Some(entry) = try_extract_balance(object)? {
            schema.address_balances.insert(&entry.0, entry.1, batch)?;
        }

        Ok(())
    }
}

#[async_trait]
impl sequential::Handler for AddressBalances {
    type Store = Store<Schema>;
    type Batch = BTreeMap<Key, u128>;

    /// Submit a write for every checkpoint, for snapshotting purposes.
    const MAX_BATCH_CHECKPOINTS: usize = 1;

    /// No batching actually happens, because `MAX_BATCH_CHECKPOINTS` is 1.
    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<(Key, u128)>) {
        batch.extend(values);
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let address_balances = &conn.store.schema().address_balances;

        for (key, val) in batch {
            if *val == 0 {
                address_balances.remove(key, &mut conn.batch)?;
                continue;
            }
            address_balances.insert(key, *val, &mut conn.batch)?;
        }

        Ok(batch.len())
    }
}

fn try_extract_balance(obj: &Object) -> anyhow::Result<Option<(Key, u128)>> {
    let Some(move_obj) = obj.data.try_as_move() else {
        return Ok(None);
    };

    let ty = move_obj.type_();

    // This the T in `Key<Balance<T>>``
    let Some(type_) = ty.balance_accumulator_field_type_maybe() else {
        return Ok(None);
    };

    let field: Field<SuiAddress, u128> = bcs::from_bytes(move_obj.contents())
        .context("Failed to deserialize balance accumulator")?;

    Ok(Some((
        Key {
            owner: field.name,
            type_,
        },
        field.value,
    )))
}
