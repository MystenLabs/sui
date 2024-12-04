// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use diesel_async::RunQueryDsl;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::{
    db,
    pipeline::{concurrent::Handler, Processor},
};
use sui_types::{
    base_types::ObjectID,
    full_checkpoint_content::CheckpointData,
    object::{Object, Owner},
};

use sui_indexer_alt_schema::{
    objects::{StoredObjInfo, StoredOwnerKind},
    schema::obj_info,
};

pub(crate) struct ObjInfo;

pub(crate) enum ProcessedObjInfoUpdate {
    Insert(Object),
    Delete(ObjectID),
}

pub(crate) struct ProcessedObjInfo {
    pub cp_sequence_number: u64,
    pub update: ProcessedObjInfoUpdate,
}

impl Processor for ObjInfo {
    const NAME: &'static str = "obj_info";
    type Value = ProcessedObjInfo;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint.checkpoint_input_objects();
        let latest_live_output_objects = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect::<BTreeMap<_, _>>();
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();
        for object_id in checkpoint_input_objects.keys() {
            if !latest_live_output_objects.contains_key(object_id) {
                // If an input object is not in the latest live output objects, it must have been deleted
                // or wrapped in this checkpoint. We keep an entry for it in the table.
                // This is necessary when we query objects and iterating over them, so that we don't
                // include the object in the result if it was deleted.
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Delete(*object_id),
                    },
                );
            }
        }
        for (object_id, object) in latest_live_output_objects.iter() {
            // If an object is newly created/unwrapped in this checkpoint, or if the owner changed,
            // we need to insert an entry for it in the table.
            let should_insert = match checkpoint_input_objects.get(object_id) {
                Some(input_object) => input_object.owner() != object.owner(),
                None => true,
            };
            if should_insert {
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Insert((*object).clone()),
                    },
                );
            }
        }

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for ObjInfo {
    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        let stored = values
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<StoredObjInfo>>>()?;
        Ok(diesel::insert_into(obj_info::table)
            .values(stored)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}

impl ProcessedObjInfo {
    pub fn object_id(&self) -> ObjectID {
        match &self.update {
            ProcessedObjInfoUpdate::Insert(object) => object.id(),
            ProcessedObjInfoUpdate::Delete(object_id) => *object_id,
        }
    }
}

impl FieldCount for ProcessedObjInfo {
    const FIELD_COUNT: usize = StoredObjInfo::FIELD_COUNT;
}

impl TryInto<StoredObjInfo> for &ProcessedObjInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StoredObjInfo> {
        match &self.update {
            ProcessedObjInfoUpdate::Insert(object) => {
                let type_ = object.type_();
                let (owner_kind, owner_id) = match object.owner() {
                    Owner::AddressOwner(a) => (StoredOwnerKind::Address, Some(a.to_vec())),
                    Owner::ObjectOwner(o) => (StoredOwnerKind::Object, Some(o.to_vec())),
                    Owner::Shared { .. } | Owner::Immutable { .. } => {
                        (StoredOwnerKind::Shared, None)
                    }
                    Owner::ConsensusV2 { authenticator, .. } => (
                        StoredOwnerKind::Address,
                        Some(authenticator.as_single_owner().to_vec()),
                    ),
                };
                Ok(StoredObjInfo {
                    object_id: object.id().to_vec(),
                    cp_sequence_number: self.cp_sequence_number as i64,
                    owner_kind: Some(owner_kind),
                    owner_id,
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
                })
            }
            ProcessedObjInfoUpdate::Delete(object_id) => Ok(StoredObjInfo {
                object_id: object_id.to_vec(),
                cp_sequence_number: self.cp_sequence_number as i64,
                owner_kind: None,
                owner_id: None,
                package: None,
                module: None,
                name: None,
                instantiation: None,
            }),
        }
    }
}
