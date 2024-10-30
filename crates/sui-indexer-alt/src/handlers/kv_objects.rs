// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use diesel_async::RunQueryDsl;
use futures::future::try_join_all;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{
    db, models::objects::StoredObject, pipeline::sequential::Handler, pipeline::Processor,
    schema::kv_objects,
};

pub struct KvObjects;

impl Processor for KvObjects {
    const NAME: &'static str = "kv_objects";
    type Value = StoredObject;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let deleted_objects = checkpoint
            .eventually_removed_object_refs_post_version()
            .into_iter()
            .map(|(id, version, _)| {
                Ok(StoredObject {
                    object_id: id.to_vec(),
                    object_version: version.value() as i64,
                    serialized_object: None,
                })
            });

        let created_objects =
            checkpoint
                .transactions
                .iter()
                .flat_map(|txn| txn.output_objects.iter())
                .map(|o| {
                    let id = o.id();
                    let version = o.version().value();
                    Ok(StoredObject {
                        object_id: id.to_vec(),
                        object_version: version as i64,
                        serialized_object: Some(bcs::to_bytes(o).with_context(|| {
                            format!("Serializing object {id} version {version}")
                        })?),
                    })
                });

        deleted_objects
            .chain(created_objects)
            .collect::<Result<Vec<_>, _>>()
    }
}

#[async_trait::async_trait]
impl Handler for KvObjects {
    const MIN_EAGER_ROWS: usize = 100;
    type Batch = Vec<Self::Value>;

    fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>) {
        batch.extend(values);
    }

    async fn commit(values: &Self::Batch, conn: &mut db::Connection<'_>) -> Result<usize> {
        let chunks = values.chunks(1000).map(|chunk| {
            diesel::insert_into(kv_objects::table)
                .values(chunk)
                .on_conflict_do_nothing()
                .execute(conn)
        });

        let inserted: usize = try_join_all(chunks).await?.into_iter().sum();
        Ok(inserted)
    }
}
