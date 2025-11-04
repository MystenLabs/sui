// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    postgres::{Connection, Db},
    store::Connection as StoreConnection,
    types::{effects::TransactionEffectsAPI, full_checkpoint_content::CheckpointData},
};
use sui_indexer_alt_schema::{objects::StoredObjVersion, schema::obj_versions};

pub(crate) struct ObjVersions;

#[async_trait]
impl Processor for ObjVersions {
    const NAME: &'static str = "obj_versions";
    type Value = StoredObjVersion;

    async fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
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

#[async_trait]
impl Handler for ObjVersions {
    type Store = Db;

    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(obj_versions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    async fn prune_v2<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        task_idx: usize,
        _total_tasks: usize,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        // spawn subtasks equal to the number of partitions on `obj_versions` table
        if task_idx > 31 {
            return Ok(0);
        }

        // TODO (wlmyng) makes me wonder if we really need to pass the from, to_exclusive?
        // i guess if we rely on framework to chunk checkpoints ...
        let global_watermark = conn
            .pruner_watermark(Self::NAME, std::time::Duration::from_secs(0))
            .await?
            .unwrap();
        let to_exclusive = global_watermark.reader_lo;
        let from = global_watermark.pruner_hi;

        let query = format!(
            "
            -- If an object was not modified within the pruning range, its latest entry is below the `from`.
            WITH latest_versions AS (
            SELECT
                object_id,
                MAX(cp_sequence_number) AS max_cp
            FROM obj_versions_p{task_idx} o
            WHERE cp_sequence_number >= {from} AND cp_sequence_number < {to_exclusive}
            GROUP BY object_id
            )
            -- Delete all older entries, including those less than the pruning range.
            DELETE FROM obj_versions_p{task_idx} o
            USING latest_versions l
            WHERE o.object_id = l.object_id
            AND o.cp_sequence_number < l.max_cp;
            ",
        );

        Ok(diesel::sql_query(query).execute(conn).await?)
    }
}
