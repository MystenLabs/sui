// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    postgres::{Connection, Db},
    types::{base_types::SuiAddress, full_checkpoint_content::Checkpoint},
};
use sui_indexer_alt_schema::{packages::StoredPackage, schema::kv_packages};
use sui_types::transaction::TransactionDataAPI;

pub(crate) struct KvPackages;

#[async_trait]
impl Processor for KvPackages {
    const NAME: &'static str = "kv_packages";

    type Value = StoredPackage;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint {
            summary,

            transactions,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = summary.sequence_number as i64;
        let mut values = vec![];
        for tx in transactions {
            for obj in tx.output_objects(&checkpoint.object_set) {
                let Some(package) = obj.data.try_as_package() else {
                    continue;
                };

                values.push(StoredPackage {
                    package_id: obj.id().to_vec(),
                    original_id: package.original_package_id().to_vec(),
                    package_version: obj.version().value() as i64,
                    is_system_package: tx.transaction.sender() == SuiAddress::ZERO,
                    serialized_object: bcs::to_bytes(obj).with_context(|| {
                        format!("Error serializing package object {}", obj.id())
                    })?,
                    cp_sequence_number,
                });
            }
        }

        Ok(values)
    }
}

#[async_trait]
impl Handler for KvPackages {
    type Store = Db;
    type Batch = Vec<Self::Value>;

    async fn commit<'a>(&self, batch: &Self::Batch, conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(kv_packages::table)
            .values(batch)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
