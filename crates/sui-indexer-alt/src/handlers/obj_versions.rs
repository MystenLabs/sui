// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::{Result, bail};
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::Processor,
    postgres::{Connection, handler::Handler},
    store::Connection as StoreConnection,
    types::{effects::TransactionEffectsAPI, full_checkpoint_content::Checkpoint},
};
use sui_indexer_alt_schema::{objects::StoredObjVersion, schema::obj_versions};

pub(crate) struct ObjVersions;

#[async_trait]
impl Processor for ObjVersions {
    const NAME: &'static str = "obj_versions";
    type Value = StoredObjVersion;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<Self::Value>> {
        let Checkpoint {
            transactions,
            summary,
            ..
        } = checkpoint.as_ref();

        let cp_sequence_number = summary.sequence_number as i64;
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
    const MIN_EAGER_ROWS: usize = 100;
    const MAX_PENDING_ROWS: usize = 10000;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        Ok(diesel::insert_into(obj_versions::table)
            .values(values)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    /// Each pruning operation determines the local latest object entries within the `[from,
    /// to_exclusive)` range. Entries older than these within the range are pruned. The previous
    /// "latest" entries before the current `pruner_hi` are also pruned. Finally, the latest entries
    /// themselves are pruned if there is a newer version between `[to_exclusive, reader_lo]`, or if
    /// the object was deleted.
    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        // If we cannot determine the global reader and pruner watermarks, we cannot proceed with
        // pruning. Return an error so the indexer can revisit this range.
        let Some(watermark) = conn
            .pruner_watermark(Self::NAME, std::time::Duration::from_secs(0))
            .await?
        else {
            bail!(
                "No pruner watermark found for {}, cannot proceed with pruning",
                Self::NAME
            );
        };

        let global_reader_lo = watermark.reader_lo as u64;
        let global_pruner_hi = watermark.pruner_hi as u64;

        // Phase 1: determine the local “latest” versions of each object within the range, and use
        // that to prune older versions between its assigned `[s, e)` range.
        let query = format!(
            "
            WITH to_del AS (
                SELECT object_id, cp_sequence_number
                FROM (
                    SELECT object_id,
                        cp_sequence_number,
                        ROW_NUMBER() OVER (
                            PARTITION BY object_id
                            ORDER BY cp_sequence_number DESC
                        ) as rn
                    FROM obj_versions
                    WHERE cp_sequence_number >= {from}
                    AND cp_sequence_number < {to_exclusive}
                ) sub
                WHERE rn > 1
            )
            DELETE FROM obj_versions o
            USING to_del d
            WHERE o.object_id = d.object_id
            AND o.cp_sequence_number = d.cp_sequence_number;
            ",
        );

        let mut rows_deleted = diesel::sql_query(query).execute(conn).await?;

        // Phase 2: using the local "latest" versions, delete each object's prior versions strictly
        // before the `global_pruner_hi`. Concurrent pruner subtasks may attempt to delete the same
        // object rows, causing transaction serialization. This is acceptable as the delete
        // operations are idempotent and converge to the same result.
        let query = format!(
            "
            WITH local_latest AS (
                SELECT DISTINCT object_id
                FROM obj_versions
                WHERE cp_sequence_number >= {from}
                AND cp_sequence_number < {to_exclusive}
            ),
            -- finds most recent version of each object strictly before `pruner_hi`
            pre_a AS (
                SELECT DISTINCT ON (o.object_id) o.object_id, o.cp_sequence_number AS pre_cp
                FROM obj_versions o
                JOIN local_latest t USING (object_id)
                WHERE o.cp_sequence_number < {global_pruner_hi}
                ORDER BY o.object_id, o.cp_sequence_number DESC
            )
            DELETE FROM obj_versions o
            USING pre_a p
            WHERE o.object_id = p.object_id
            AND o.cp_sequence_number = p.pre_cp;
            "
        );
        rows_deleted += diesel::sql_query(query).execute(conn).await?;

        // Phase 3: Delete the latest references themselves if there is at least one newer version
        // between `[to_exclusive, global_reader_lo]`.
        let query = format!(
            "
            WITH local_latest AS (
                SELECT object_id, MAX(cp_sequence_number) AS head_cp
                FROM obj_versions
                WHERE cp_sequence_number >= {from}
                AND cp_sequence_number < {to_exclusive}
                GROUP BY object_id
            ),
            to_del AS (
                SELECT o.object_id, o.cp_sequence_number
                FROM obj_versions o
                JOIN local_latest h USING (object_id)
                WHERE o.cp_sequence_number = h.head_cp
                AND (
                    o.object_digest IS NULL
                    OR
                    EXISTS (
                        SELECT 1
                        FROM obj_versions n
                        WHERE n.object_id = o.object_id
                            AND n.cp_sequence_number > o.cp_sequence_number
                            AND n.cp_sequence_number <= {global_reader_lo}
                        LIMIT 1
                    )
                )
            )
            DELETE FROM obj_versions o
            USING to_del d
            WHERE o.object_id = d.object_id
            AND o.cp_sequence_number = d.cp_sequence_number;
            "
        );

        rows_deleted += diesel::sql_query(query).execute(conn).await?;

        Ok(rows_deleted)
    }
}

#[cfg(test)]
mod tests {
    use diesel::{ExpressionMethods, query_dsl::methods::FilterDsl};
    use sui_indexer_alt_framework::{
        Indexer, store::CommitterWatermark,
        types::test_checkpoint_data_builder::TestCheckpointBuilder,
    };
    use sui_indexer_alt_schema::MIGRATIONS;

    use super::*;

    // A helper function to return all entries in the obj_versions table sorted by object_id and
    // cp_sequence_number.
    async fn get_all_obj_versions(conn: &mut Connection<'_>) -> Result<Vec<StoredObjVersion>> {
        let query = obj_versions::table.load(conn).await?;
        Ok(query)
    }

    async fn get_obj_versions_for(
        conn: &mut Connection<'_>,
        object_idx: u64,
    ) -> Result<Vec<StoredObjVersion>> {
        let object_id = TestCheckpointBuilder::derive_object_id(object_idx);
        Ok(obj_versions::table
            .filter(obj_versions::object_id.eq(object_id.to_vec()))
            .load(conn)
            .await?)
    }

    /// In the case where the reader and pruner watermarks cannot be retrieved, the pruning
    /// operation should fail.
    #[tokio::test]
    async fn test_prune_without_reader_watermark() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let result = ObjVersions.prune(0, 10, &mut conn).await;
        assert!(result.is_err());
    }

    /// Test that all intermediate versions of an object within the pruning range are removed,
    /// leaving the local latest version. If there are newer versions beyond the pruning range,
    /// prune the local latest too.
    #[tokio::test]
    async fn test_prune_phase_one_and_three() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointBuilder::new(0);

        // Checkpoint 0
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint0))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 1
        builder = builder
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint1))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 2
        builder = builder
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint2))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 3 - note the new object created here, to differentiate.
        builder = builder
            .start_transaction(0)
            .create_owned_object(1)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint3))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 3,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(ObjVersions::NAME, 3)
            .await
            .unwrap();

        let rows_pruned = ObjVersions.prune(0, 3, &mut conn).await.unwrap();
        let all_obj_versions = get_all_obj_versions(&mut conn).await.unwrap();
        let obj_0 = all_obj_versions
            .iter()
            .find(|v| v.object_id[0] == 0)
            .unwrap();
        let obj_1 = all_obj_versions
            .iter()
            .find(|v| v.object_id[0] == 1)
            .unwrap();
        let gas_obj = all_obj_versions
            .iter()
            .find(|v| v.object_id[0] != 0 && v.object_id[0] != 1)
            .unwrap();

        // 2 rows for 0x0, 2 rows for the gas object, and a third row for gas object at checkpoint
        // 2.
        assert_eq!(rows_pruned, 5);
        assert_eq!(all_obj_versions.len(), 3);
        // Demonstrates phase 1: 0x0 last modified in checkpoint 2.
        assert_eq!(obj_0.cp_sequence_number as u64, 2);
        // 0x1 created in checkpoint 3.
        assert_eq!(obj_1.cp_sequence_number as u64, 3);
        // Demonstrates phase 3: between the pruning range `[0, 3)`, the local latest version is at
        // checkpoint 2. But since same gas object was used again in checkpoint 3, there is a newer
        // version at checkpoint 3 and thus the local latest is pruned.
        assert_eq!(gas_obj.cp_sequence_number as u64, 3);
    }

    /// When the pruning range slides up, previous latest versions before the new `pruner_hi` are
    /// likely superseded by new modifications in the new range. Test that obsolete versions are
    /// pruned, and unmodified objects remain.
    #[tokio::test]
    async fn test_prune_phase_two() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointBuilder::new(0);

        // Checkpoint 0 - create objs 0x0 and 0x1. 0x0 will remain unmodified, 0x1 will be modified
        // at checkpoint 3.
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint0))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 1 - nothing happens.
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint1))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 2 - 0x1 is modified.
        builder = builder
            .start_transaction(0)
            .mutate_owned_object(1)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint2))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 3 - nothing happens.
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint3))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 3,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Emulate the indexer advancing the reader watermark to 1, pruning `[0, 1)`, then bumping
        // `reader_lo` to 2 and `pruner_hi` to 1.
        conn.set_reader_watermark(ObjVersions::NAME, 1)
            .await
            .unwrap();
        let rows_pruned = ObjVersions.prune(0, 1, &mut conn).await.unwrap();
        // Only the gas object is pruned. 0x0 is never modified again after checkpoint 0, and 0x1 is
        // not modified until checkpoint 2. From the perspective of the `reader_lo` at checkpoint 1,
        // both objects have their latest versions at checkpoint 0.
        assert_eq!(rows_pruned, 1);
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();
        assert_eq!(obj0_versions.len(), 1);
        assert_eq!(obj0_versions[0].cp_sequence_number as u64, 0);
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        assert_eq!(obj1_versions.len(), 2);
        assert_eq!(obj1_versions[0].cp_sequence_number as u64, 0);
        assert_eq!(obj1_versions[1].cp_sequence_number as u64, 2);
        let all_obj_versions = get_all_obj_versions(&mut conn).await.unwrap();
        let objs_at_cp0 = all_obj_versions
            .iter()
            .filter(|v| v.cp_sequence_number as u64 == 0)
            .count();
        assert_eq!(objs_at_cp0, 2);
        conn.set_pruner_watermark(ObjVersions::NAME, 1)
            .await
            .unwrap();
        conn.set_reader_watermark(ObjVersions::NAME, 2)
            .await
            .unwrap();

        // Prune `[1, 2)`, then bump `reader_lo` to 3 and `pruner_hi` to 2.
        let rows_pruned = ObjVersions.prune(1, 2, &mut conn).await.unwrap();
        // Just the gas object entry is pruned again.
        assert_eq!(rows_pruned, 1);
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();
        assert_eq!(obj0_versions.len(), 1);
        assert_eq!(obj0_versions[0].cp_sequence_number as u64, 0);
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        assert_eq!(obj1_versions.len(), 2);
        assert_eq!(obj1_versions[0].cp_sequence_number as u64, 0);
        assert_eq!(obj1_versions[1].cp_sequence_number as u64, 2);
        conn.set_pruner_watermark(ObjVersions::NAME, 2)
            .await
            .unwrap();
        conn.set_reader_watermark(ObjVersions::NAME, 3)
            .await
            .unwrap();

        // Test phase 2 by pruning `[2, 3)`, then bump `reader_lo` to 3 and `pruner_hi` to 2.
        let rows_pruned = ObjVersions.prune(2, 3, &mut conn).await.unwrap();
        // Both the gas object and 0x1 at checkpoint 0 will be pruned.
        assert_eq!(rows_pruned, 2);
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();
        assert_eq!(obj0_versions.len(), 1);
        assert_eq!(obj0_versions[0].cp_sequence_number as u64, 0);
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        assert_eq!(obj1_versions.len(), 1);
        assert_eq!(obj1_versions[0].cp_sequence_number as u64, 2);
        let all_obj_versions = get_all_obj_versions(&mut conn).await.unwrap();
        assert_eq!(all_obj_versions.len(), 3);
    }

    #[tokio::test]
    async fn test_wrap_and_prune_before_unwrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointBuilder::new(0);

        // Checkpoint 0
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint0))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 1 - wrap 0x0
        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint1))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 2 - filler checkpoint
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint2))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 2,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        conn.set_reader_watermark(ObjVersions::NAME, 2)
            .await
            .unwrap();
        ObjVersions.prune(0, 2, &mut conn).await.unwrap();
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();
        assert_eq!(obj0_versions.len(), 0);

        // Checkpoint 3 - unwrap 0x0
        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint3))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 3,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(ObjVersions::NAME, 3)
            .await
            .unwrap();
        conn.set_pruner_watermark(ObjVersions::NAME, 2)
            .await
            .unwrap();
        ObjVersions.prune(2, 3, &mut conn).await.unwrap();
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();

        assert_eq!(obj0_versions.len(), 1);
    }

    #[tokio::test]
    async fn test_wrap_and_prune_after_unwrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointBuilder::new(0);

        // Checkpoint 0
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint0))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 1 - wrap 0x0
        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint1))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 2 - filler checkpoint
        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint2))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        // Checkpoint 3 - unwrap 0x0
        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        ObjVersions::commit(
            ObjVersions
                .process(&Arc::new(checkpoint3))
                .await
                .unwrap()
                .as_ref(),
            &mut conn,
        )
        .await
        .unwrap();

        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 3,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        conn.set_reader_watermark(ObjVersions::NAME, 3)
            .await
            .unwrap();
        ObjVersions.prune(0, 3, &mut conn).await.unwrap();
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();

        assert_eq!(obj0_versions.len(), 1);
    }

    #[tokio::test]
    async fn test_out_of_order_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointBuilder::new(0);

        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint0)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 1)
            .transfer_object(2, 1)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint1)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .transfer_object(1, 0)
            .transfer_object(2, 0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint2)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint3)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 3,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(ObjVersions::NAME, 3)
            .await
            .unwrap();

        let pre_prune_all_obj_versions = get_all_obj_versions(&mut conn).await.unwrap();
        ObjVersions.prune(2, 3, &mut conn).await.unwrap();
        let all_obj_versions = get_all_obj_versions(&mut conn).await.unwrap();
        assert_eq!(pre_prune_all_obj_versions.len(), all_obj_versions.len());

        ObjVersions.prune(1, 2, &mut conn).await.unwrap();
        let all_obj_versions = get_all_obj_versions(&mut conn).await.unwrap();
        for obj in all_obj_versions {
            assert!(obj.cp_sequence_number as u64 != 1);
        }

        ObjVersions.prune(0, 1, &mut conn).await.unwrap();
        let all_obj_versions = get_all_obj_versions(&mut conn).await.unwrap();
        for obj in all_obj_versions {
            assert!(obj.cp_sequence_number as u64 > 0);
        }
    }

    #[tokio::test]
    async fn test_out_of_order_pruning_nonzero_pruner_hi() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointBuilder::new(0);

        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint0)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint1)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .mutate_owned_object(2)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint2)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        // Pivot on checkpoint 2 - `pruner_hi` will be 2.

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint3)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(1, 1)
            .finish_transaction();
        let checkpoint4 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint4)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(2, 1)
            .finish_transaction();
        let checkpoint5 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint5)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 5,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(ObjVersions::NAME, 5)
            .await
            .unwrap();
        conn.set_pruner_watermark(ObjVersions::NAME, 3)
            .await
            .unwrap();

        ObjVersions.prune(5, 6, &mut conn).await.unwrap();
        // Object 0x2 was created in checkpoint 1, modified in checkpoint 2, and transferred in
        // checkpoint 5. When pruning [5, 6), only the latest previous version is deleted, at
        // checkpoint 2, leaving the entry at checkpoint 1 and 5. Note that the pruner does not
        // delete all entries before the global `pruner_hi`.
        let obj2_versions = get_obj_versions_for(&mut conn, 2).await.unwrap();
        assert_eq!(obj2_versions.len(), 2);
        assert_eq!(obj2_versions[0].cp_sequence_number as u64, 1);
        assert_eq!(obj2_versions[1].cp_sequence_number as u64, 5);
        // Other objects untouched in this checkpoint range should not be pruned.
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        assert_eq!(obj1_versions.len(), 2);

        ObjVersions.prune(4, 5, &mut conn).await.unwrap();
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        // Since 0x1 was transferred in checkpoint 4, created in checkpoint 1, and the pruner_hi
        // pivot is at checkpoint 2, the previous entry at checkpoint 1 will be pruned.
        assert_eq!(obj1_versions.len(), 1);
        // Other objects untouched in this checkpoint range should not be pruned.
        let obj2_versions = get_obj_versions_for(&mut conn, 2).await.unwrap();
        assert_eq!(obj2_versions.len(), 2);

        ObjVersions.prune(3, 4, &mut conn).await.unwrap();
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();
        assert_eq!(obj0_versions.len(), 1);
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        assert_eq!(obj1_versions.len(), 1);
        let obj2_versions = get_obj_versions_for(&mut conn, 2).await.unwrap();
        assert_eq!(obj2_versions.len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_pruning() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let mut builder = TestCheckpointBuilder::new(0);

        // Create the same scenario as the out-of-order test
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .create_owned_object(1)
            .create_owned_object(2)
            .finish_transaction();
        let checkpoint0 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint0)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .transfer_object(1, 1)
            .transfer_object(2, 1)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint1)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .transfer_object(1, 0)
            .transfer_object(2, 0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint2)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        builder = builder.start_transaction(0).finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        let result = ObjVersions.process(&Arc::new(checkpoint3)).await.unwrap();
        ObjVersions::commit(&result, &mut conn).await.unwrap();

        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();
        assert_eq!(obj0_versions.len(), 3);
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        assert_eq!(obj1_versions.len(), 3);
        let obj2_versions = get_obj_versions_for(&mut conn, 2).await.unwrap();
        assert_eq!(obj2_versions.len(), 3);

        let mut handles = Vec::new();
        let store = indexer.store().clone();
        let store1 = store.clone();
        let store2 = store.clone();
        let store3 = store.clone();
        conn.set_committer_watermark(
            ObjVersions::NAME,
            CommitterWatermark {
                checkpoint_hi_inclusive: 3,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        conn.set_reader_watermark(ObjVersions::NAME, 2)
            .await
            .unwrap();
        handles.push(tokio::spawn(async move {
            let mut conn = store1.connect().await.unwrap();
            ObjVersions.prune(0, 1, &mut conn).await.unwrap()
        }));
        handles.push(tokio::spawn(async move {
            let mut conn = store2.connect().await.unwrap();
            ObjVersions.prune(1, 2, &mut conn).await.unwrap()
        }));
        handles.push(tokio::spawn(async move {
            let mut conn = store3.connect().await.unwrap();
            ObjVersions.prune(2, 3, &mut conn).await.unwrap()
        }));
        // Wait for all pruning operations to complete
        futures::future::join_all(handles).await;
        let obj0_versions = get_obj_versions_for(&mut conn, 0).await.unwrap();
        let obj1_versions = get_obj_versions_for(&mut conn, 1).await.unwrap();
        let obj2_versions = get_obj_versions_for(&mut conn, 2).await.unwrap();

        assert_eq!(obj0_versions.len(), 1);
        assert_eq!(obj0_versions[0].cp_sequence_number as u64, 2);
        assert_eq!(obj1_versions.len(), 1);
        assert_eq!(obj1_versions[0].cp_sequence_number as u64, 2);
        assert_eq!(obj2_versions.len(), 1);
        assert_eq!(obj2_versions[0].cp_sequence_number as u64, 2);
    }
}
