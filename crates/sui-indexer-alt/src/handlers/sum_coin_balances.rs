// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};

use anyhow::{anyhow, bail, ensure};
use diesel::{upsert::excluded, ExpressionMethods};
use diesel_async::RunQueryDsl;
use diesel_update_from::update_from;
use futures::future::{try_join_all, Either};
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::pipeline::{sequential::Handler, Processor};
use sui_indexer_alt_schema::{
    objects::{StoredObjectUpdate, StoredSumCoinBalance, UpdateKind},
    schema::sum_coin_balances,
};
use sui_pg_db as db;
use sui_types::{
    base_types::ObjectID, effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData,
    object::Owner,
};

const MAX_UPDATE_CHUNK_ROWS: usize = i16::MAX as usize / StoredSumCoinBalance::FIELD_COUNT;
const MAX_DELETE_CHUNK_ROWS: usize = i16::MAX as usize;

pub(crate) struct SumCoinBalances;

impl Processor for SumCoinBalances {
    const NAME: &'static str = "sum_coin_balances";

    type Value = StoredObjectUpdate<StoredSumCoinBalance>;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
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

            // Do a fist pass to add updates without their associated contents into the `values`
            // mapping, based on the transaction's object changes.
            for change in tx.effects.object_changes() {
                let object_id = change.id;
                let lamport_version = tx.effects.lamport_version().value();

                // Object is not a coin, so not relevant for this processor.
                if !coin_types.contains_key(&object_id) {
                    continue;
                }

                let (kind, object_version) = match (change.input_version, change.output_version) {
                    // Unwrapped and deleted
                    (None, None) => continue,

                    // Unwrapped or created
                    (None, Some(object_version)) => (UpdateKind::Insert, object_version.value()),

                    // Wrapped or deleted
                    (Some(_), None) => (UpdateKind::Delete, lamport_version),

                    // Mutated
                    (Some(_), Some(object_version)) => (UpdateKind::Update, object_version.value()),
                };

                let entry = match values.entry(object_id) {
                    Entry::Vacant(entry) => entry,
                    Entry::Occupied(entry) => {
                        ensure!(entry.get().object_version > object_version);
                        continue;
                    }
                };

                entry.insert(StoredObjectUpdate {
                    kind,
                    object_id,
                    object_version,
                    cp_sequence_number,
                    value: None,
                });
            }

            // Then, do a second pass to fill out contents for created and updated objects.
            for object in &tx.output_objects {
                let object_id = object.id();
                let object_version = object.version().value();

                let Some(coin_type) = coin_types.get(&object_id) else {
                    continue;
                };

                let Some(update) = values.get_mut(&object_id) else {
                    bail!(
                        "Missing update for output object {}, in transaction {}",
                        object_id.to_canonical_display(/* with_prefix */ true),
                        tx.transaction.digest(),
                    );
                };

                // Update from a later transaction, no need to fill its contents.
                if update.object_version > object_version {
                    continue;
                }

                ensure!(
                    update.kind != UpdateKind::Delete,
                    "Deleted coin {} appears in outputs for transaction {}",
                    object_id.to_canonical_display(/* with_prefix */ true),
                    tx.transaction.digest(),
                );

                if let Owner::AddressOwner(owner_id) = object.owner() {
                    let Some(coin) = object.as_coin_maybe() else {
                        bail!("Failed to deserialize Coin for {object_id}");
                    };

                    update.value = Some(StoredSumCoinBalance {
                        object_id: object_id.to_vec(),
                        object_version: object_version as i64,
                        owner_id: owner_id.to_vec(),
                        coin_type: coin_type.clone(),
                        coin_balance: coin.balance.value() as i64,
                    });
                } else {
                    // The coin exists but is no longer address owned, so get rid of it from this
                    // table.
                    update.kind = UpdateKind::Delete;
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
        // `updates` are guaranteed to be provided in checkpoint order, so overwriting them in the
        // batch will result in it containing the most up-to-date update for each object, but when
        // overwriting, we need to handle the `kind` field carefully, to not miss an insert or a
        // delete.
        for mut update in updates {
            match batch.entry(update.object_id) {
                Entry::Vacant(entry) => {
                    entry.insert(update);
                }

                Entry::Occupied(mut entry) => {
                    use UpdateKind as K;
                    match (entry.get().kind, update.kind) {
                        (K::Insert, K::Delete) => {
                            entry.remove();
                        }

                        (K::Insert, K::Insert | K::Update) => {
                            update.kind = K::Insert;
                            entry.insert(update);
                        }

                        (K::Update | K::Delete, K::Delete) => {
                            update.kind = K::Delete;
                            entry.insert(update);
                        }

                        (K::Update | K::Delete, K::Insert | K::Update) => {
                            update.kind = K::Update;
                            entry.insert(update);
                        }
                    }
                }
            }
        }
    }

    async fn commit(batch: &Self::Batch, conn: &mut db::Connection<'_>) -> anyhow::Result<usize> {
        let mut inserts = vec![];
        let mut updates = vec![];
        let mut deletes = vec![];

        for update in batch.values() {
            let object_id = update.object_id;
            let object_version = update.object_version;
            match (&update.kind, &update.value) {
                (UpdateKind::Insert, Some(value)) => inserts.push(value.clone()),
                (UpdateKind::Update, Some(value)) => updates.push(value.clone()),
                (UpdateKind::Delete, _) => deletes.push(object_id.to_vec()),

                (UpdateKind::Insert | UpdateKind::Update, None) => {
                    bail!(
                        "Missing contents for coin {} version {}",
                        object_id.to_canonical_display(/* with_prefix */ true),
                        object_version,
                    );
                }
            }
        }

        let insert_chunks = inserts
            .chunks(MAX_UPDATE_CHUNK_ROWS)
            .map(|i| Either::Left(Either::Left(i)));

        let update_chunks = updates
            .chunks(MAX_UPDATE_CHUNK_ROWS)
            .map(|u| Either::Left(Either::Right(u)));

        let delete_chunks = deletes.chunks(MAX_DELETE_CHUNK_ROWS).map(Either::Right);

        let futures = insert_chunks
            .chain(update_chunks)
            .chain(delete_chunks)
            .map(|chunk| match chunk {
                Either::Left(Either::Left(insert)) => Either::Left(Either::Left(
                    diesel::insert_into(sum_coin_balances::table)
                        .values(insert)
                        .execute(conn),
                )),
                Either::Left(Either::Right(update)) => Either::Left(Either::Right(
                    update_from(sum_coin_balances::table)
                        .values(update)
                        .set((
                            sum_coin_balances::object_version
                                .eq(excluded(sum_coin_balances::object_version)),
                            sum_coin_balances::owner_id.eq(excluded(sum_coin_balances::owner_id)),
                            sum_coin_balances::coin_balance
                                .eq(excluded(sum_coin_balances::coin_balance)),
                        ))
                        .filter(
                            sum_coin_balances::object_id.eq(excluded(sum_coin_balances::object_id)),
                        )
                        .execute(conn),
                )),

                Either::Right(delete) => Either::Right(
                    diesel::delete(sum_coin_balances::table)
                        .filter(sum_coin_balances::object_id.eq_any(delete.iter().cloned()))
                        .execute(conn),
                ),
            });

        Ok(try_join_all(futures).await?.into_iter().sum())
    }
}
