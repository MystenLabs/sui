// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::ObjectType;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes object types to BigTable.
/// Wrap with `BigTableHandler` for the full `concurrent::Handler` implementation.
pub struct ObjectTypesPipeline;

#[async_trait::async_trait]
impl Processor for ObjectTypesPipeline {
    const NAME: &'static str = "kvstore_object_types";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = Vec::with_capacity(checkpoint.object_set.len());

        for obj in checkpoint.object_set.iter() {
            let object_type = ObjectType::from(obj);
            let entry = tables::make_entry(
                tables::object_types::encode_key(&obj.id()),
                tables::object_types::encode(&object_type)?,
                Some(timestamp_ms),
            );
            entries.push(entry);
        }

        Ok(entries)
    }
}

impl BigTableProcessor for ObjectTypesPipeline {
    const TABLE: &'static str = tables::object_types::NAME;
}
