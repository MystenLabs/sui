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
    objects::{StoredObjectUpdate, StoredOwnerKind, StoredSumObjType, UpdateKind},
    schema::sum_obj_types,
};
use sui_pg_db as db;
use sui_types::{
    base_types::ObjectID, effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData,
    object::Owner,
};

const MAX_UPDATE_CHUNK_ROWS: usize = i16::MAX as usize / StoredSumObjType::FIELD_COUNT;
const MAX_DELETE_CHUNK_ROWS: usize = i16::MAX as usize;

pub(crate) struct SumObjTypes;

impl Processor for SumObjTypes {
    const NAME: &'static str = "sum_obj_types";

    type Value = StoredObjectUpdate<StoredSumObjType>;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = checkpoint_summary.sequence_number;
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();

        // Iterate over transactions in reverse so we see the latest version of each object first.
        for tx in transactions.iter().rev() {
            // Do a first pass to add updates without their associated contents into the `values`
            // mapping, based on the transaction's object changes.
            for change in tx.effects.object_changes() {
                let object_id = change.id;
                let lamport_version = tx.effects.lamport_version().value();

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

            // Then, do a second pass to fill out the contents for created and updated objects.
            for object in &tx.output_objects {
                let object_id = object.id();
                let object_version = object.version().value();
                let Some(update) = values.get_mut(&object_id) else {
                    bail!(
                        "Missing object change for output object {}, in transaction {}",
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
                    "Deleted object {} appears in outputs for transaction {}",
                    object_id.to_canonical_display(/* with_prefix */ true),
                    tx.transaction.digest(),
                );

                let type_ = object.type_();
                update.value = Some(StoredSumObjType {
                    object_id: object_id.to_vec(),
                    object_version: object_version as i64,

                    owner_kind: match object.owner() {
                        Owner::AddressOwner(_) => StoredOwnerKind::Address,
                        Owner::ObjectOwner(_) => StoredOwnerKind::Object,
                        Owner::Shared { .. } => StoredOwnerKind::Shared,
                        Owner::Immutable => StoredOwnerKind::Immutable,
                        // TODO: Implement support for ConsensusV2 objects.
                        Owner::ConsensusV2 { .. } => todo!(),
                    },

                    owner_id: match object.owner() {
                        Owner::AddressOwner(a) => Some(a.to_vec()),
                        Owner::ObjectOwner(o) => Some(o.to_vec()),
                        _ => None,
                    },

                    package: type_.map(|t| t.address().to_vec()),
                    module: type_.map(|t| t.module().to_string()),
                    name: type_.map(|t| t.name().to_string()),
                    instantiation: type_
                        .map(|t| bcs::to_bytes(&t.type_params()))
                        .transpose()
                        .map_err(|e| {
                            anyhow!(
                                "Failed to serialize type parameters for {}: {e}",
                                object.id().to_canonical_display(/* with_prefix */ true),
                            )
                        })?,
                });
            }
        }

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for SumObjTypes {
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

    async fn commit(values: &Self::Batch, conn: &mut db::Connection<'_>) -> anyhow::Result<usize> {
        let mut inserts = vec![];
        let mut updates = vec![];
        let mut deletes = vec![];

        for update in values.values() {
            let object_id = update.object_id;
            let object_version = update.object_version;

            match (&update.kind, &update.value) {
                (UpdateKind::Insert, Some(value)) => inserts.push(value.clone()),
                (UpdateKind::Update, Some(value)) => updates.push(value.clone()),
                (UpdateKind::Delete, None) => deletes.push(update.object_id.to_vec()),

                (UpdateKind::Insert | UpdateKind::Update, None) => {
                    bail!(
                        "Missing contents for object {} version {}",
                        object_id.to_canonical_display(/* with_prefix */ true),
                        object_version,
                    );
                }

                (UpdateKind::Delete, Some(_)) => {
                    bail!(
                        "Unexpected contents for deleted object {} version {}",
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
                    diesel::insert_into(sum_obj_types::table)
                        .values(insert)
                        .execute(conn),
                )),
                Either::Left(Either::Right(update)) => Either::Left(Either::Right(
                    update_from(sum_obj_types::table)
                        .values(update)
                        .set((
                            sum_obj_types::object_version
                                .eq(excluded(sum_obj_types::object_version)),
                            sum_obj_types::owner_kind.eq(excluded(sum_obj_types::owner_kind)),
                            sum_obj_types::owner_id.eq(excluded(sum_obj_types::owner_id)),
                        ))
                        .filter(sum_obj_types::object_id.eq(excluded(sum_obj_types::object_id)))
                        .execute(conn),
                )),

                Either::Right(delete) => Either::Right(
                    diesel::delete(sum_obj_types::table)
                        .filter(sum_obj_types::object_id.eq_any(delete.iter().cloned()))
                        .execute(conn),
                ),
            });

        Ok(try_join_all(futures).await?.into_iter().sum())
    }
}
