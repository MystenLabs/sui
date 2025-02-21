// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use diesel::{upsert::excluded, ExpressionMethods};
use diesel_async::RunQueryDsl;
use futures::future::try_join_all;
use sui_field_count::FieldCount;
use sui_indexer_alt_framework::pipeline::{sequential::Handler, Processor};
use sui_indexer_alt_schema::{packages::StoredPackage, schema::sum_packages};
use sui_pg_db as db;
use sui_types::full_checkpoint_content::CheckpointData;

const MAX_INSERT_CHUNK_ROWS: usize = i16::MAX as usize / StoredPackage::FIELD_COUNT;

pub(crate) struct SumPackages;

impl Processor for SumPackages {
    const NAME: &'static str = "sum_packages";

    type Value = StoredPackage;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            checkpoint_summary,
            transactions,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = checkpoint_summary.sequence_number as i64;
        let mut values = vec![];
        for tx in transactions {
            for obj in &tx.output_objects {
                let Some(package) = obj.data.try_as_package() else {
                    continue;
                };

                values.push(StoredPackage {
                    package_id: obj.id().to_vec(),
                    original_id: package.original_package_id().to_vec(),
                    package_version: obj.version().value() as i64,
                    move_package: bcs::to_bytes(package)
                        .map_err(|e| anyhow!("Error serializing package {}: {e}", obj.id()))?,
                    cp_sequence_number,
                });
            }
        }

        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for SumPackages {
    type Batch = BTreeMap<Vec<u8>, StoredPackage>;

    fn batch(batch: &mut Self::Batch, values: Vec<Self::Value>) {
        for value in values {
            batch.insert(value.package_id.clone(), value);
        }
    }

    async fn commit(batch: &Self::Batch, conn: &mut db::Connection<'_>) -> Result<usize> {
        let values: Vec<_> = batch.values().cloned().collect();
        let updates = values.chunks(MAX_INSERT_CHUNK_ROWS).map(|chunk| {
            diesel::insert_into(sum_packages::table)
                .values(chunk)
                .on_conflict(sum_packages::package_id)
                .do_update()
                .set((
                    sum_packages::package_version.eq(excluded(sum_packages::package_version)),
                    sum_packages::move_package.eq(excluded(sum_packages::move_package)),
                    sum_packages::cp_sequence_number.eq(excluded(sum_packages::cp_sequence_number)),
                ))
                .execute(conn)
        });

        Ok(try_join_all(updates).await?.into_iter().sum())
    }
}
