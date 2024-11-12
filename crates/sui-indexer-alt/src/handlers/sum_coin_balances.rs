// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};

use anyhow::{anyhow, bail, ensure};
use diesel::{upsert::excluded, ExpressionMethods};
use diesel_async::RunQueryDsl;
use futures::future::try_join_all;
use sui_types::{
    base_types::ObjectID, effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData,
    object::Owner,
};

use crate::{
    db,
    models::objects::{StoredObjectUpdate, StoredSumCoinBalance},
    pipeline::{sequential::Handler, Processor},
    schema::sum_coin_balances,
};

/// Each insert or update will include at most this many rows -- the size is chosen to maximize the
/// rows without hitting the limit on bind parameters.
const UPDATE_CHUNK_ROWS: usize = i16::MAX as usize / 5;

/// Each deletion will include at most this many rows.
const DELETE_CHUNK_ROWS: usize = i16::MAX as usize;

pub struct SumCoinBalances;

impl Processor for SumCoinBalances {
    const NAME: &'static str = "sum_coin_balances";

    type Value = StoredObjectUpdate<StoredSumCoinBalance>;

    fn process(checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = checkpoint_summary.sequence_number;
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();
        let mut coin_types: BTreeMap<ObjectID, Vec<u8>> = BTreeMap::new();

        // Iterate over transactions in reverse so we see the latest version of each object first.
        for tx in transactions.iter().rev() {
            // Find all coins in the transaction's inputs and outputs.
            for object in tx.input_objects.iter().chain(tx.output_objects.iter()) {
                if let Some(coin_type) = object.type_().and_then(|t| t.coin_type_maybe()) {
                    let serialized = bcs::to_bytes(&coin_type)
                        .map_err(|_| anyhow!("Failed to serialize type for {}", object.id()))?;

                    coin_types.insert(object.id(), serialized);
                }
            }

            // Deleted and wrapped coins
            for change in tx.effects.object_changes() {
                // The object is not deleted/wrapped, or if it is it was unwrapped in the same
                // transaction.
                if change.output_digest.is_some() || change.input_version.is_none() {
                    continue;
                }

                // Object is not a coin
                if !coin_types.contains_key(&change.id) {
                    continue;
                }

                let object_id = change.id;
                let object_version = tx.effects.lamport_version().value();
                match values.entry(object_id) {
                    Entry::Occupied(entry) => {
                        ensure!(entry.get().object_version > object_version);
                    }

                    Entry::Vacant(entry) => {
                        entry.insert(StoredObjectUpdate {
                            object_id,
                            object_version,
                            cp_sequence_number,
                            update: None,
                        });
                    }
                }
            }

            // Modified and created coins.
            for object in &tx.output_objects {
                let object_id = object.id();
                let object_version = object.version().value();

                let Some(coin_type) = coin_types.get(&object_id) else {
                    continue;
                };

                // Coin balance only tracks address-owned objects
                let Owner::AddressOwner(owner_id) = object.owner() else {
                    continue;
                };

                let Some(coin) = object.as_coin_maybe() else {
                    bail!("Failed to deserialize Coin for {object_id}");
                };

                match values.entry(object_id) {
                    Entry::Occupied(entry) => {
                        ensure!(entry.get().object_version > object_version);
                    }

                    Entry::Vacant(entry) => {
                        entry.insert(StoredObjectUpdate {
                            object_id,
                            object_version,
                            cp_sequence_number,
                            update: Some(StoredSumCoinBalance {
                                object_id: object_id.to_vec(),
                                object_version: object_version as i64,
                                owner_id: owner_id.to_vec(),
                                coin_type: coin_type.clone(),
                                coin_balance: coin.balance.value() as i64,
                            }),
                        });
                    }
                }
            }
        }

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for SumCoinBalances {
    type Batch = BTreeMap<ObjectID, Self::Value>;

    fn batch(batch: &mut Self::Batch, updates: Vec<Self::Value>) {
        // `updates` are guaranteed to be provided in checkpoint order, so blindly inserting them
        // will result in the batch containing the most up-to-date update for each object.
        for update in updates {
            batch.insert(update.object_id, update);
        }
    }

    async fn commit(batch: &Self::Batch, conn: &mut db::Connection<'_>) -> anyhow::Result<usize> {
        let mut updates = vec![];
        let mut deletes = vec![];

        for update in batch.values() {
            if let Some(update) = &update.update {
                updates.push(update.clone());
            } else {
                deletes.push(update.object_id.to_vec());
            }
        }

        let update_chunks = updates.chunks(UPDATE_CHUNK_ROWS).map(|chunk| {
            diesel::insert_into(sum_coin_balances::table)
                .values(chunk)
                .on_conflict(sum_coin_balances::object_id)
                .do_update()
                .set((
                    sum_coin_balances::object_version
                        .eq(excluded(sum_coin_balances::object_version)),
                    sum_coin_balances::owner_id.eq(excluded(sum_coin_balances::owner_id)),
                    sum_coin_balances::coin_balance.eq(excluded(sum_coin_balances::coin_balance)),
                ))
                .execute(conn)
        });

        let updated: usize = try_join_all(update_chunks).await?.into_iter().sum();

        let delete_chunks = deletes.chunks(DELETE_CHUNK_ROWS).map(|chunk| {
            diesel::delete(sum_coin_balances::table)
                .filter(sum_coin_balances::object_id.eq_any(chunk.iter().cloned()))
                .execute(conn)
        });

        let deleted: usize = try_join_all(delete_chunks).await?.into_iter().sum();

        Ok(updated + deleted)
    }
}
