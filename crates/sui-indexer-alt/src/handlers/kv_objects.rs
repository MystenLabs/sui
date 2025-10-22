// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    postgres::{Connection, Db},
    types::full_checkpoint_content::Checkpoint,
};
use sui_indexer_alt_schema::{objects::StoredObject, schema::kv_objects};
use sui_types::effects::TransactionEffectsAPI;

pub(crate) struct KvObjects;

#[async_trait]
impl Processor for KvObjects {
    const NAME: &'static str = "kv_objects";
    type Value = StoredObject;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let deleted_objects = checkpoint.transactions.iter().flat_map(|txn| {
            txn.effects.deleted().into_iter().map(|object_ref| {
                Ok(StoredObject {
                    object_id: object_ref.0.to_vec(),
                    object_version: object_ref.1.value() as i64,
                    serialized_object: None,
                })
            })
        });

        let created_objects = checkpoint.transactions.iter().flat_map(|txn| {
            txn.output_objects(&checkpoint.object_set).map(|o| {
                let id = o.id();
                let version = o.version();
                Ok(StoredObject {
                    object_id: id.to_vec(),
                    object_version: version.value() as i64,
                    serialized_object: Some(bcs::to_bytes(o).with_context(|| {
                        format!("Serializing object {id} version {}", version.value())
                    })?),
                })
            })
        });

        deleted_objects
            .chain(created_objects)
            .collect::<Result<Vec<_>, _>>()
    }
}

#[async_trait]
impl Handler for KvObjects {
    type Store = Db;

    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(kv_objects::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
