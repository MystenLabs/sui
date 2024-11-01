// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel_async::RunQueryDsl;
use sui_types::full_checkpoint_content::CheckpointData;

use crate::{
    db,
    models::objects::StoredObjVersion,
    pipeline::{concurrent::Handler, Processor},
    schema::obj_versions,
};

pub struct ObjVersions;

impl Processor for ObjVersions {
    const NAME: &'static str = "obj_versions";
    type Value = StoredObjVersion;

    fn process(checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = checkpoint_summary.sequence_number as i64;
        Ok(transactions
            .iter()
            .flat_map(|txn| txn.output_objects.iter())
            .map(|o| {
                let id = o.id();
                let version = o.version().value();
                let digest = o.digest();
                StoredObjVersion {
                    object_id: id.to_vec(),
                    object_version: version as i64,
                    object_digest: digest.inner().into(),
                    cp_sequence_number,
                }
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl Handler for ObjVersions {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_CHUNK_ROWS: usize = 1000;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(obj_versions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}
