// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};

use anyhow::{anyhow, ensure};
use diesel::{upsert::excluded, ExpressionMethods};
use diesel_async::RunQueryDsl;
use futures::future::{try_join_all, Either};
use sui_types::{
    base_types::ObjectID, effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData,
    object::Owner,
};

use crate::{
    db,
    models::objects::{StoredObjectUpdate, StoredOwnerKind, StoredSumObjType},
    pipeline::{sequential::Handler, Processor},
    schema::sum_obj_types,
};

/// Each insert or update will include at most this many rows -- the size is chosen to maximize the
/// rows without hitting the limit on bind parameters.
const UPDATE_CHUNK_ROWS: usize = i16::MAX as usize / 8;

/// Each deletion will include at most this many rows.
const DELETE_CHUNK_ROWS: usize = i16::MAX as usize;

pub struct SumObjTypes;

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
            // Deleted and wrapped objects -- objects that show up without a digest in
            // `object_changes` are either deleted or wrapped. Objects without an input version
            // must have been unwrapped and deleted, meaning they do not need to be deleted from
            // our records.
            for change in tx.effects.object_changes() {
                if change.output_digest.is_some() || change.input_version.is_none() {
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

            // Modified and created objects.
            for object in &tx.output_objects {
                let object_id = object.id();
                let object_version = object.version().value();
                match values.entry(object_id) {
                    Entry::Occupied(entry) => {
                        ensure!(entry.get().object_version > object_version);
                    }

                    Entry::Vacant(entry) => {
                        let type_ = object.type_();
                        entry.insert(StoredObjectUpdate {
                            object_id,
                            object_version,
                            cp_sequence_number,
                            update: Some(StoredSumObjType {
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
                                            object
                                                .id()
                                                .to_canonical_display(/* with_prefix */ true),
                                        )
                                    })?,
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
impl Handler for SumObjTypes {
    type Batch = BTreeMap<ObjectID, Self::Value>;

    fn batch(batch: &mut Self::Batch, updates: Vec<Self::Value>) {
        // `updates` are guaranteed to be provided in checkpoint order, so blindly inserting them
        // will result in the batch containing the most up-to-date update for each object.
        for update in updates {
            batch.insert(update.object_id, update);
        }
    }

    async fn commit(values: &Self::Batch, conn: &mut db::Connection<'_>) -> anyhow::Result<usize> {
        let mut updates = vec![];
        let mut deletes = vec![];

        for update in values.values() {
            if let Some(update) = &update.update {
                updates.push(update.clone());
            } else {
                deletes.push(update.object_id.to_vec());
            }
        }

        let update_chunks = updates.chunks(UPDATE_CHUNK_ROWS).map(Either::Left);
        let delete_chunks = deletes.chunks(DELETE_CHUNK_ROWS).map(Either::Right);

        let futures = update_chunks.chain(delete_chunks).map(|chunk| match chunk {
            Either::Left(update) => Either::Left(
                diesel::insert_into(sum_obj_types::table)
                    .values(update)
                    .on_conflict(sum_obj_types::object_id)
                    .do_update()
                    .set((
                        sum_obj_types::object_version.eq(excluded(sum_obj_types::object_version)),
                        sum_obj_types::owner_kind.eq(excluded(sum_obj_types::owner_kind)),
                        sum_obj_types::owner_id.eq(excluded(sum_obj_types::owner_id)),
                    ))
                    .execute(conn),
            ),

            Either::Right(delete) => Either::Right(
                diesel::delete(sum_obj_types::table)
                    .filter(sum_obj_types::object_id.eq_any(delete.iter().cloned()))
                    .execute(conn),
            ),
        });

        Ok(try_join_all(futures).await?.into_iter().sum())
    }
}
