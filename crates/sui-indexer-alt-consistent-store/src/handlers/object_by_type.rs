// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use sui_indexer_alt_framework::{
    pipeline::{sequential, Processor},
    types::{base_types::VersionDigest, full_checkpoint_content::CheckpointData, object::Object},
};

use crate::{
    restore::Restore,
    schema::{object_by_type::Key, Schema},
    store::{Connection, Store},
};

use super::{checkpoint_input_objects, checkpoint_output_objects};

pub(crate) struct ObjectByType;

pub enum Value {
    Put(Key, VersionDigest),
    Del(Key),
}

#[async_trait]
impl Processor for ObjectByType {
    const NAME: &'static str = "object_by_type";
    type Value = Value;

    async fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Value>> {
        let input_objects = checkpoint_input_objects(checkpoint)?;
        let output_objects = checkpoint_output_objects(checkpoint)?;
        let mut values = vec![];

        // Objects that are in the inputs but not the outputs have been deleted.
        for (id, &(input, _)) in &input_objects {
            let Some(key_in) = Key::from_object(input) else {
                continue;
            };

            if !output_objects.contains_key(id) {
                values.push(Value::Del(key_in));
            }
        }

        for (id, (output, digest)) in output_objects {
            let Some(key_out) = Key::from_object(output) else {
                continue;
            };

            // If the ID is in the input objects with a different key, it needs to be deleted at
            // that location.
            if let Some(key_in) = input_objects
                .get(&id)
                .and_then(|(input, _)| Key::from_object(input))
            {
                if key_in != key_out {
                    values.push(Value::Del(key_in));
                }
            }

            // The object is always put at its output location.
            values.push(Value::Put(key_out, (output.version(), digest)));
        }

        Ok(values)
    }
}

impl Restore<Schema> for ObjectByType {
    fn restore(
        schema: &Schema,
        object: &Object,
        batch: &mut rocksdb::WriteBatch,
    ) -> anyhow::Result<()> {
        if let Some(key) = Key::from_object(object) {
            let val = (object.version(), object.digest());
            schema.object_by_type.insert(key, val, batch)?;
        }

        Ok(())
    }
}

#[async_trait]
impl sequential::Handler for ObjectByType {
    type Store = Store<Schema>;
    type Batch = Vec<Value>;

    /// Submit a write for every checkpoint, for snapshotting purposes.
    const MAX_BATCH_CHECKPOINTS: usize = 1;

    /// No batching actually happens, because `MAX_BATCH_CHECKPOINTS` is 1.
    fn batch(batch: &mut Self::Batch, values: Vec<Value>) {
        batch.extend(values);
    }

    async fn commit<'a>(
        batch: &Self::Batch,
        conn: &mut Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let object_by_type = &conn.store.schema().object_by_type;

        for value in batch {
            match value {
                Value::Put(key, val) => {
                    object_by_type.insert(key, val, &mut conn.batch)?;
                }
                Value::Del(key) => {
                    object_by_type.remove(key, &mut conn.batch)?;
                }
            }
        }

        Ok(batch.len())
    }
}
