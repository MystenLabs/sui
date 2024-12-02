// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use diesel_async::RunQueryDsl;
use sui_types::{base_types::ObjectID, full_checkpoint_content::CheckpointData, object::Owner};

use crate::{
    db,
    models::objects::{StoredObjInfo, StoredOwnerKind},
    pipeline::{concurrent::Handler, Processor},
    schema::obj_info,
};

pub struct ObjInfo;

impl Processor for ObjInfo {
    const NAME: &'static str = "obj_info";
    type Value = StoredObjInfo;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number as i64;
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
                    StoredObjInfo {
                        object_id: object_id.to_vec(),
                        cp_sequence_number,
                        owner_kind: None,
                        owner_id: None,
                        package: None,
                        module: None,
                        name: None,
                        instantiation: None,
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
                let type_ = object.type_();
                values.insert(
                    *object_id,
                    StoredObjInfo {
                        object_id: object_id.to_vec(),
                        cp_sequence_number,
                        owner_kind: Some(match object.owner() {
                            Owner::AddressOwner(_) => StoredOwnerKind::Address,
                            Owner::ObjectOwner(_) => StoredOwnerKind::Object,
                            Owner::Shared { .. } => StoredOwnerKind::Shared,
                            Owner::Immutable => StoredOwnerKind::Immutable,
                            Owner::ConsensusV2 { .. } => todo!(),
                        }),

                        owner_id: match object.owner() {
                            Owner::AddressOwner(a) => Some(a.to_vec()),
                            Owner::ObjectOwner(o) => Some(o.to_vec()),
                            Owner::Shared { .. } | Owner::Immutable { .. } => None,
                            Owner::ConsensusV2 { .. } => todo!(),
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
        Ok(diesel::insert_into(obj_info::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
