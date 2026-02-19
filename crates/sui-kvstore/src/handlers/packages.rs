// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework::pipeline::Processor;
use sui_types::base_types::SuiAddress;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::transaction::TransactionDataAPI;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::handlers::BigTableProcessor;
use crate::tables;

/// Pipeline that writes package metadata to the `packages` table.
pub struct PackagesPipeline;

#[async_trait::async_trait]
impl Processor for PackagesPipeline {
    const NAME: &'static str = "kvstore_packages";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.summary.sequence_number;
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = vec![];

        for txn in &checkpoint.transactions {
            let is_system_package = txn.transaction.sender() == SuiAddress::ZERO;
            for obj in txn.output_objects(&checkpoint.object_set) {
                let Some(package) = obj.data.try_as_package() else {
                    continue;
                };

                let original_id = package.original_package_id().to_vec();
                let package_id = obj.id().to_vec();
                let version = obj.version().value();

                let entry = tables::make_entry(
                    tables::packages::encode_key(&original_id, version),
                    tables::packages::encode(cp_sequence_number, &package_id, is_system_package),
                    Some(timestamp_ms),
                );
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

impl BigTableProcessor for PackagesPipeline {
    const TABLE: &'static str = tables::packages::NAME;
    const FANOUT: usize = 100;
}
