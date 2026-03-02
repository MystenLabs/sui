// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes package_id → original_id mappings to the `packages_by_id` table.
pub struct PackagesByIdPipeline;

#[async_trait::async_trait]
impl Processor for PackagesByIdPipeline {
    const NAME: &'static str = "kvstore_packages_by_id";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = vec![];

        for txn in &checkpoint.transactions {
            for obj in txn.output_objects(&checkpoint.object_set) {
                let Some(package) = obj.data.try_as_package() else {
                    continue;
                };

                let original_id = package.original_package_id().to_vec();
                let package_id = obj.id().to_vec();

                let entry = tables::make_entry(
                    tables::packages_by_id::encode_key(&package_id),
                    tables::packages_by_id::encode(&original_id),
                    Some(timestamp_ms),
                );
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

impl BigTableProcessor for PackagesByIdPipeline {
    const TABLE: &'static str = tables::packages_by_id::NAME;
}
