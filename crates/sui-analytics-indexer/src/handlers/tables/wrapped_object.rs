// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::EpochId;
use sui_types::full_checkpoint_content::Checkpoint;

use super::{get_move_struct, parse_struct};
use crate::Row;
use crate::package_store::PackageCache;
use crate::tables::WrappedObjectRow;

pub struct WrappedObjectProcessor {
    package_cache: Arc<PackageCache>,
}

impl WrappedObjectProcessor {
    pub fn new(package_cache: Arc<PackageCache>) -> Self {
        Self { package_cache }
    }
}

impl Row for WrappedObjectRow {
    fn get_epoch(&self) -> EpochId {
        self.epoch
    }

    fn get_checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

#[async_trait]
impl Processor for WrappedObjectProcessor {
    const NAME: &'static str = "wrapped_object";
    const FANOUT: usize = 10;
    type Value = WrappedObjectRow;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let epoch = checkpoint.summary.data().epoch;
        let checkpoint_seq = checkpoint.summary.data().sequence_number;
        let timestamp_ms = checkpoint.summary.data().timestamp_ms;

        let mut wrapped_objects = Vec::new();

        for transaction in &checkpoint.transactions {
            for object in transaction.output_objects(&checkpoint.object_set) {
                let move_struct = if let Some((tag, contents)) = object
                    .struct_tag()
                    .and_then(|tag| object.data.try_as_move().map(|mo| (tag, mo.contents())))
                {
                    match get_move_struct(
                        &tag,
                        contents,
                        &self.package_cache.resolver_for_epoch(epoch),
                    )
                    .await
                    {
                        Ok(move_struct) => Some(move_struct),
                        Err(err)
                            if err
                                .downcast_ref::<sui_types::object::bounded_visitor::Error>()
                                .filter(|e| {
                                    matches!(
                                        e,
                                        sui_types::object::bounded_visitor::Error::OutOfBudget
                                    )
                                })
                                .is_some() =>
                        {
                            tracing::warn!(
                                "Skipping struct with type {} because it was too large.",
                                tag
                            );
                            None
                        }
                        Err(err) => return Err(err),
                    }
                } else {
                    None
                };

                let mut object_wrapped_structs = BTreeMap::new();
                if let Some(move_struct) = move_struct {
                    parse_struct("$", move_struct, &mut object_wrapped_structs);
                }

                for (json_path, wrapped_struct) in object_wrapped_structs.iter() {
                    let row = WrappedObjectRow {
                        object_id: wrapped_struct.object_id.map(|id| id.to_string()),
                        root_object_id: object.id().to_string(),
                        root_object_version: object.version().value(),
                        checkpoint: checkpoint_seq,
                        epoch,
                        timestamp_ms,
                        json_path: json_path.to_string(),
                        struct_tag: wrapped_struct.struct_tag.clone().map(|tag| tag.to_string()),
                    };
                    wrapped_objects.push(row);
                }
            }
        }

        Ok(wrapped_objects)
    }
}
