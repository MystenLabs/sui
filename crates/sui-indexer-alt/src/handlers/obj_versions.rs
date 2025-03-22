// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    db,
    pipeline::{concurrent::Handler, Processor},
    types::{effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData},
};
use sui_indexer_alt_schema::{objects::StoredObjVersion, schema::obj_versions};

pub(crate) struct ObjVersions;

/// A temporary pipeline to backfill deleted/wrapped records.
pub(crate) struct ObjVersionsSentinelBackfill;

impl Processor for ObjVersions {
    const NAME: &'static str = "obj_versions";
    type Value = StoredObjVersion;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let CheckpointData {
            transactions,
            checkpoint_summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = checkpoint_summary.sequence_number as i64;
        Ok(transactions
            .iter()
            .flat_map(|tx| {
                let lamport = tx.effects.lamport_version();

                tx.effects
                    .object_changes()
                    .into_iter()
                    .map(move |c| StoredObjVersion {
                        object_id: c.id.to_vec(),
                        // If the object was created or modified, it has an output version,
                        // otherwise it was deleted/wrapped and its version is the transaction's
                        // lamport version.
                        object_version: c.output_version.unwrap_or(lamport).value() as i64,
                        object_digest: c.output_digest.map(|d| d.inner().into()),
                        cp_sequence_number,
                    })
            })
            .collect())
    }
}

impl Processor for ObjVersionsSentinelBackfill {
    const NAME: &'static str = "obj_versions_sentinel_backfill";
    type Value = StoredObjVersion;

    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let mut values = ObjVersions.process(checkpoint)?;
        values.retain(|v| v.object_digest.is_none());
        Ok(values)
    }
}

#[async_trait::async_trait]
impl Handler for ObjVersions {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        Ok(diesel::insert_into(obj_versions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }
}

#[async_trait::async_trait]
impl Handler for ObjVersionsSentinelBackfill {
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit(values: &[Self::Value], conn: &mut db::Connection<'_>) -> Result<usize> {
        ObjVersions::commit(values, conn).await
    }
}
