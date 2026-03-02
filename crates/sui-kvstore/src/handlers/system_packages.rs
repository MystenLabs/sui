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

/// Pipeline that writes system package entries (packages published by address 0x0).
pub struct SystemPackagesPipeline;

#[async_trait::async_trait]
impl Processor for SystemPackagesPipeline {
    const NAME: &'static str = "kvstore_system_packages";
    type Value = Entry;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.summary.sequence_number;
        let timestamp_ms = checkpoint.summary.timestamp_ms;
        let mut entries = vec![];

        for txn in &checkpoint.transactions {
            if txn.transaction.sender() != SuiAddress::ZERO {
                continue;
            }

            for obj in txn.output_objects(&checkpoint.object_set) {
                let Some(package) = obj.data.try_as_package() else {
                    continue;
                };

                let original_id = package.original_package_id().to_vec();

                let entry = tables::make_entry(
                    tables::system_packages::encode_key(&original_id),
                    tables::system_packages::encode(cp_sequence_number),
                    Some(timestamp_ms),
                );
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

impl BigTableProcessor for SystemPackagesPipeline {
    const TABLE: &'static str = tables::system_packages::NAME;
    const MIN_EAGER_ROWS: usize = 1;
}
