// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::storage::ObjectKey;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes objects to BigTable.
/// Wrap with `BigTableHandler` for the full `concurrent::Handler` implementation.
pub struct ObjectsPipeline;

#[async_trait::async_trait]
impl Processor for ObjectsPipeline {
    const NAME: &'static str = "kvstore_objects";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        self.process_sync(checkpoint)
    }
}

impl BigTableProcessor for ObjectsPipeline {
    const TABLE: &'static str = tables::objects::NAME;
    const MAX_PENDING_ROWS: usize = 50_000;

    fn process_sync(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = vec![];

        for txn in &checkpoint.transactions {
            for object in txn.output_objects(&checkpoint.object_set) {
                let object_key = ObjectKey(object.id(), object.version());
                let entry = tables::make_entry(
                    tables::objects::encode_key(&object_key),
                    tables::objects::encode(object)?,
                    Some(timestamp_ms),
                );
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}
