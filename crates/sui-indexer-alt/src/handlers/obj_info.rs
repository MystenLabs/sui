// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_indexer_alt_framework::{
    pipeline::{concurrent::Handler, Processor},
    postgres::{Connection, Db},
    types::{base_types::ObjectID, full_checkpoint_content::CheckpointData, object::Object},
    FieldCount,
};
use sui_indexer_alt_schema::{objects::StoredObjInfo, schema::obj_info};

use crate::consistent_pruning::{PruningInfo, PruningLookupTable};

use super::checkpoint_input_objects;

#[derive(Default)]
pub(crate) struct ObjInfo {
    pruning_lookup_table: Arc<PruningLookupTable>,
}

pub(crate) enum ProcessedObjInfoUpdate {
    Insert(Object),
    Delete(ObjectID),
}

pub(crate) struct ProcessedObjInfo {
    pub cp_sequence_number: u64,
    pub update: ProcessedObjInfoUpdate,
}

impl Processor for ObjInfo {
    const NAME: &'static str = "obj_info";
    type Value = ProcessedObjInfo;

    // TODO: Add tests for this function and the pruner.
    fn process(&self, checkpoint: &Arc<CheckpointData>) -> Result<Vec<Self::Value>> {
        let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        let checkpoint_input_objects = checkpoint_input_objects(checkpoint)?;
        let latest_live_output_objects = checkpoint
            .latest_live_output_objects()
            .into_iter()
            .map(|o| (o.id(), o))
            .collect::<BTreeMap<_, _>>();
        let mut values: BTreeMap<ObjectID, Self::Value> = BTreeMap::new();
        let mut prune_info = PruningInfo::new();
        for object_id in checkpoint_input_objects.keys() {
            if !latest_live_output_objects.contains_key(object_id) {
                // If an input object is not in the latest live output objects, it must have been deleted
                // or wrapped in this checkpoint. We keep an entry for it in the table.
                // This is necessary when we query objects and iterating over them, so that we don't
                // include the object in the result if it was deleted.
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Delete(*object_id),
                    },
                );
                prune_info.add_deleted_object(*object_id);
            }
        }
        for (object_id, object) in latest_live_output_objects.iter() {
            // If an object is newly created/unwrapped in this checkpoint, or if the owner changed,
            // we need to insert an entry for it in the table.
            let should_insert = match checkpoint_input_objects.get(object_id) {
                Some(input_object) => input_object.owner() != object.owner(),
                None => true,
            };
            if should_insert {
                values.insert(
                    *object_id,
                    ProcessedObjInfo {
                        cp_sequence_number,
                        update: ProcessedObjInfoUpdate::Insert((*object).clone()),
                    },
                );
                // We do not need to prune if the object was created in this checkpoint,
                // because this object would not have been in the table prior to this checkpoint.
                if checkpoint_input_objects.contains_key(object_id) {
                    prune_info.add_mutated_object(*object_id);
                }
            }
        }
        self.pruning_lookup_table
            .insert(cp_sequence_number, prune_info);

        Ok(values.into_values().collect())
    }
}

#[async_trait::async_trait]
impl Handler for ObjInfo {
    type Store = Db;

    const PRUNING_REQUIRES_PROCESSED_VALUES: bool = true;

    async fn commit<'a>(values: &[Self::Value], conn: &mut Connection<'a>) -> Result<usize> {
        let stored = values
            .iter()
            .map(|v| v.try_into())
            .collect::<Result<Vec<StoredObjInfo>>>()?;
        Ok(diesel::insert_into(obj_info::table)
            .values(stored)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?)
    }

    // TODO: Add tests for this function.
    async fn prune<'a>(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection<'a>,
    ) -> Result<usize> {
        use sui_indexer_alt_schema::schema::obj_info::dsl;

        let to_prune = self
            .pruning_lookup_table
            .get_prune_info(from, to_exclusive)?;

        if to_prune.is_empty() {
            self.pruning_lookup_table.gc_prune_info(from, to_exclusive);
            return Ok(0);
        }

        // For each (object_id, cp_sequence_number), find and delete its immediate predecessor
        let values = to_prune
            .iter()
            .map(|(object_id, seq_number)| {
                let object_id_hex = hex::encode(object_id);
                format!("('\\x{}'::BYTEA, {}::BIGINT)", object_id_hex, seq_number)
            })
            .collect::<Vec<_>>()
            .join(",");

        let query = format!(
            "
            WITH modifications(object_id, cp_sequence_number) AS (
                VALUES {}
            )
            DELETE FROM obj_info oi
            USING modifications m
            WHERE oi.{:?} = m.object_id
              AND oi.{:?} = (
                SELECT oi2.cp_sequence_number
                FROM obj_info oi2
                WHERE oi2.{:?} = m.object_id
                  AND oi2.{:?} < m.cp_sequence_number
                ORDER BY oi2.cp_sequence_number DESC
                LIMIT 1
              )
            ",
            values,
            dsl::object_id,
            dsl::cp_sequence_number,
            dsl::object_id,
            dsl::cp_sequence_number,
        );

        let rows_deleted = sql_query(query).execute(conn).await?;
        self.pruning_lookup_table.gc_prune_info(from, to_exclusive);
        Ok(rows_deleted)
    }
}

impl FieldCount for ProcessedObjInfo {
    const FIELD_COUNT: usize = StoredObjInfo::FIELD_COUNT;
}

impl TryInto<StoredObjInfo> for &ProcessedObjInfo {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<StoredObjInfo> {
        match &self.update {
            ProcessedObjInfoUpdate::Insert(object) => {
                StoredObjInfo::from_object(object, self.cp_sequence_number as i64)
            }
            ProcessedObjInfoUpdate::Delete(object_id) => Ok(StoredObjInfo {
                object_id: object_id.to_vec(),
                cp_sequence_number: self.cp_sequence_number as i64,
                owner_kind: None,
                owner_id: None,
                package: None,
                module: None,
                name: None,
                instantiation: None,
                obsolete_at: None,
                marked_predecessor: false,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use sui_indexer_alt_framework::{
        postgres::{self},
        types::{
            base_types::{dbg_addr, SequenceNumber},
            object::Owner,
            test_checkpoint_data_builder::TestCheckpointDataBuilder,
        },
        Indexer,
    };
    use sui_indexer_alt_schema::{objects::StoredOwnerKind, MIGRATIONS};

    use super::*;

    #[derive(diesel::QueryableByName)]
    struct T0Result {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        pred_deleted: i64,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        pred_obsolete: i64,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        marked_predecessor: i64,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        start_timestamp_ms: i64,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        end_timestamp_ms: i64,
    }

    #[derive(diesel::QueryableByName)]
    struct T1Result {
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        deleted_latest_count: i64,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        deleted_obsolete_count: i64,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        start_timestamp_ms: i64,
        #[diesel(sql_type = diesel::sql_types::BigInt)]
        end_timestamp_ms: i64,
    }

    #[derive(Debug, Clone)]
    struct TimelineEvent {
        timestamp_ms: i64,
        label: String,
        event_type: EventType,
        details: String,
    }

    #[derive(Debug, Clone)]
    enum EventType {
        Start,
        End,
    }

    struct TimelineDiagram {
        events: Vec<TimelineEvent>,
    }

    impl TimelineDiagram {
        fn new() -> Self {
            Self { events: Vec::new() }
        }

        fn add_transaction_events(&mut self, result: &T0Result, tx_name: &str) {
            self.events.push(TimelineEvent {
                timestamp_ms: result.start_timestamp_ms,
                label: format!("{}_START", tx_name),
                event_type: EventType::Start,
                details: format!("Transaction {} begins", tx_name),
            });

            self.events.push(TimelineEvent {
                timestamp_ms: result.end_timestamp_ms,
                label: format!("{}_END", tx_name),
                event_type: EventType::End,
                details: format!(
                    "pred_deleted: {}, pred_obsolete: {}, marked_predecessor: {}",
                    result.pred_deleted, result.pred_obsolete, result.marked_predecessor
                ),
            });
        }

        fn add_t1_events(&mut self, result: &T1Result, tx_name: &str) {
            self.events.push(TimelineEvent {
                timestamp_ms: result.start_timestamp_ms,
                label: format!("{}_START", tx_name),
                event_type: EventType::Start,
                details: "T1 cleanup begins".to_string(),
            });

            self.events.push(TimelineEvent {
                timestamp_ms: result.end_timestamp_ms,
                label: format!("{}_END", tx_name),
                event_type: EventType::End,
                details: format!(
                    "deleted_latest_count: {}, deleted_obsolete_count: {}",
                    result.deleted_latest_count, result.deleted_obsolete_count
                ),
            });
        }

        fn generate_timeline(&mut self) -> String {
            // Sort events by timestamp
            self.events.sort_by_key(|e| e.timestamp_ms);

            if self.events.is_empty() {
                return "No events recorded".to_string();
            }

            let start_time = self.events[0].timestamp_ms;
            let mut timeline = String::new();

            timeline.push_str("=== TRANSACTION TIMELINE ===\n");
            timeline.push_str(&format!("Base timestamp: {}\n\n", start_time));

            // Create visual timeline
            let relative_times: Vec<(i64, &TimelineEvent)> = self
                .events
                .iter()
                .map(|e| (e.timestamp_ms - start_time, e))
                .collect();

            // ASCII timeline representation
            timeline.push_str("Timeline (ms from start):\n");
            timeline.push_str("0ms ────────────────────────────────────────────────────→ time\n");

            for (relative_time, event) in &relative_times {
                let indent = " ".repeat((*relative_time as usize / 10).min(50)); // Scale for display
                let marker = match event.event_type {
                    EventType::Start => "┌─",
                    EventType::End => "└─",
                };

                timeline.push_str(&format!(
                    "{}{}{}ms: {} ({})\n",
                    indent, marker, relative_time, event.label, event.details
                ));
            }

            timeline.push_str("\n");

            // Execution pattern analysis
            timeline.push_str("=== EXECUTION PATTERN ===\n");
            let pattern = self.analyze_execution_pattern();
            timeline.push_str(&format!("Pattern: {}\n", pattern));

            // Overlap analysis
            let overlap_info = self.analyze_overlaps();
            timeline.push_str(&format!("Overlap: {}\n\n", overlap_info));

            // Event sequence
            timeline.push_str("=== EVENT SEQUENCE ===\n");
            for (i, event) in self.events.iter().enumerate() {
                timeline.push_str(&format!(
                    "{}. {}ms: {} - {}\n",
                    i + 1,
                    event.timestamp_ms - start_time,
                    event.label,
                    event.details
                ));
            }

            timeline
        }

        fn analyze_execution_pattern(&self) -> String {
            let events: Vec<&str> = self.events.iter().map(|e| e.label.as_str()).collect();

            // Look for key patterns
            let pattern = events.join(" → ");

            // Classify the pattern
            if pattern.contains("CiT0_END") && pattern.contains("CjT0_START") {
                let cit0_end_pos = events.iter().position(|&x| x == "CiT0_END").unwrap_or(0);
                let cjt0_start_pos = events.iter().position(|&x| x == "CjT0_START").unwrap_or(0);

                if cit0_end_pos < cjt0_start_pos {
                    format!("SEQUENTIAL (CiT0 → CjT0) [{}]", pattern)
                } else {
                    format!("CONCURRENT [{}]", pattern)
                }
            } else {
                format!("CONCURRENT [{}]", pattern)
            }
        }

        fn analyze_overlaps(&self) -> String {
            let cit0_start = self.events.iter().find(|e| e.label == "CiT0_START");
            let cit0_end = self.events.iter().find(|e| e.label == "CiT0_END");
            let cjt0_start = self.events.iter().find(|e| e.label == "CjT0_START");
            let cjt0_end = self.events.iter().find(|e| e.label == "CjT0_END");

            match (cit0_start, cit0_end, cjt0_start, cjt0_end) {
                (Some(ci_start), Some(ci_end), Some(cj_start), Some(cj_end)) => {
                    let ci_duration = ci_end.timestamp_ms - ci_start.timestamp_ms;
                    let cj_duration = cj_end.timestamp_ms - cj_start.timestamp_ms;

                    // Check for overlap
                    let overlap_start = ci_start.timestamp_ms.max(cj_start.timestamp_ms);
                    let overlap_end = ci_end.timestamp_ms.min(cj_end.timestamp_ms);

                    if overlap_start < overlap_end {
                        let overlap_duration = overlap_end - overlap_start;
                        format!(
                            "{}ms overlap (CiT0: {}ms, CjT0: {}ms)",
                            overlap_duration, ci_duration, cj_duration
                        )
                    } else {
                        format!(
                            "No overlap (CiT0: {}ms, CjT0: {}ms)",
                            ci_duration, cj_duration
                        )
                    }
                }
                _ => "Incomplete timing data".to_string(),
            }
        }
    }

    // A helper function to return all entries in the obj_info table sorted by object_id and
    // cp_sequence_number.
    async fn get_all_obj_info(conn: &mut Connection<'_>) -> Result<Vec<StoredObjInfo>> {
        let query = obj_info::table.load(conn).await?;
        Ok(query)
    }

    async fn t0(
        conn: &mut Connection<'_>,
        from: u64,
        to_exclusive: u64,
        predecessors_sleep: Option<u64>,
        pred_deleted_sleep: Option<u64>,
        pred_obsolete_sleep: Option<u64>,
        marked_predecessor_sleep: Option<u64>,
        _label: String,
    ) -> Result<T0Result> {
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let query = postgres::sql_query!(
            "
            WITH start_timing AS (
                SELECT (extract(epoch from clock_timestamp()) * 1000)::BIGINT as start_ts
            ),
        predecessors AS (
            SELECT
                latest.object_id as latest_object_id,
                latest.cp_sequence_number as latest_cp_sequence_number,
                latest.marked_predecessor as latest_marked_predecessor,
                pred.object_id as pred_object_id,
                pred.cp_sequence_number as pred_cp_sequence_number,
                pred.marked_predecessor as pred_marked_predecessor,
                pg_sleep({BigInt}) as predecessors_sleep
            FROM obj_info latest
            LEFT JOIN LATERAL (
                SELECT object_id, cp_sequence_number, marked_predecessor
                FROM obj_info p
                WHERE p.object_id = latest.object_id
                  AND p.cp_sequence_number < latest.cp_sequence_number
                ORDER BY p.cp_sequence_number DESC
                LIMIT 1
            ) pred ON true
            WHERE latest.cp_sequence_number >= {BigInt} AND latest.cp_sequence_number < {BigInt}
            ORDER BY latest.object_id, latest.cp_sequence_number
            FOR UPDATE OF latest
        )
        -- Delete preds that already marked their own immediate predecessors
        -- And intermediate entries among 'latest' changes
        , pred_deleted AS (
    DELETE FROM obj_info
    WHERE (object_id, cp_sequence_number) IN (
        (SELECT pred_object_id, pred_cp_sequence_number
        FROM predecessors
        WHERE pred_marked_predecessor = true)

        UNION

        (SELECT latest_object_id, latest_cp_sequence_number
        FROM predecessors p1
        WHERE EXISTS (
            SELECT 1 FROM predecessors p2
            WHERE p2.pred_object_id = p1.latest_object_id
            AND p2.pred_cp_sequence_number = p1.latest_cp_sequence_number
        ))
    )
    RETURNING object_id
)
        , pred_deleted_sleep AS (
            SELECT pg_sleep({BigInt}) as dummy
        )
        -- Otherwise, flag them for later deletion
        , pred_obsolete AS (
            UPDATE obj_info
            SET obsolete_at = p.latest_cp_sequence_number
            FROM predecessors p
            WHERE obj_info.object_id = p.pred_object_id
              AND obj_info.cp_sequence_number = p.pred_cp_sequence_number
              AND p.pred_marked_predecessor = false
            RETURNING obj_info.object_id
        )
        , pred_obsolete_sleep AS (
            SELECT pg_sleep({BigInt}) as dummy
        )
        -- Finally, mark 'latest' rows to have marked their own immediate predecessors
        , marked_predecessor AS (
            UPDATE obj_info
            SET marked_predecessor = true
            WHERE (object_id, cp_sequence_number) IN (
                SELECT latest_object_id, latest_cp_sequence_number FROM predecessors
                ORDER BY latest_object_id, latest_cp_sequence_number
            )
            RETURNING object_id
        )
        , marked_predecessor_sleep AS (
            SELECT pg_sleep({BigInt}) as dummy
        )
        SELECT
            (SELECT COUNT(*)::BIGINT FROM pred_deleted) as pred_deleted,
            (SELECT COUNT(*)::BIGINT FROM pred_obsolete) as pred_obsolete,
            (SELECT COUNT(*)::BIGINT FROM marked_predecessor) as marked_predecessor,
            (SELECT start_ts FROM start_timing) as start_timestamp_ms,
            (SELECT extract(epoch from clock_timestamp()) * 1000)::BIGINT as end_timestamp_ms;
        ",
            predecessors_sleep.unwrap_or(0) as i64,
            from as i64,
            to_exclusive as i64,
            pred_deleted_sleep.unwrap_or(0) as i64,
            pred_obsolete_sleep.unwrap_or(0) as i64,
            marked_predecessor_sleep.unwrap_or(0) as i64,
        );

        let mut result: T0Result = query.get_result(conn).await?;
        let end = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        result.end_timestamp_ms = end;
        result.start_timestamp_ms = start;
        Ok(result)
    }

    async fn t0_v2(
        conn: &mut Connection<'_>,
        from: u64,
        to_exclusive: u64,
        predecessors_sleep: Option<u64>,
        pred_deleted_sleep: Option<u64>,
        pred_obsolete_sleep: Option<u64>,
        marked_predecessor_sleep: Option<u64>,
        _label: String,
    ) -> Result<T0Result> {
        use diesel_async::AsyncConnection;

        let result = conn.transaction::<_, diesel::result::Error, _>(move | conn| {
            Box::pin(
            async move {
                let start = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;

                if let Some(sleep) = predecessors_sleep {
                    postgres::sql_query!("SELECT pg_sleep({BigInt})", sleep as i64)
                        .execute(conn).await?;
                }

                #[derive(diesel::QueryableByName)]
                struct PredecessorRow {
                    #[diesel(sql_type = diesel::sql_types::Bytea)]
                    latest_object_id: Vec<u8>,
                    #[diesel(sql_type = diesel::sql_types::BigInt)]
                    latest_cp_sequence_number: i64,
                    #[diesel(sql_type = diesel::sql_types::Bool)]
                    latest_marked_predecessor: bool,
                    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Bytea>)]
                    pred_object_id: Option<Vec<u8>>,
                    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::BigInt>)]
                    pred_cp_sequence_number: Option<i64>,
                    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Bool>)]
                    pred_marked_predecessor: Option<bool>,
                }

                let predecessors = postgres::sql_query!(
                    "
                    SELECT
                        latest.object_id as latest_object_id,
                        latest.cp_sequence_number as latest_cp_sequence_number,
                        latest.marked_predecessor as latest_marked_predecessor,
                        pred.object_id as pred_object_id,
                        pred.cp_sequence_number as pred_cp_sequence_number,
                        pred.marked_predecessor as pred_marked_predecessor
                    FROM obj_info latest
                    LEFT JOIN LATERAL (
                        SELECT object_id, cp_sequence_number, marked_predecessor
                        FROM obj_info p
                        WHERE p.object_id = latest.object_id
                          AND p.cp_sequence_number < latest.cp_sequence_number
                        ORDER BY p.cp_sequence_number DESC
                        LIMIT 1
                    ) pred ON true
                    WHERE latest.cp_sequence_number >= {BigInt} AND latest.cp_sequence_number < {BigInt}
                    ORDER BY latest.object_id, latest.cp_sequence_number
                    FOR UPDATE OF latest
                    ",
                    from as i64,
                    to_exclusive as i64,
                ).load::<PredecessorRow>(conn).await?;

                // Step 2: Delete predecessors that already marked their predecessors
                // AND intermediate entries
                if let Some(sleep) = pred_deleted_sleep {
                    postgres::sql_query!("SELECT pg_sleep({BigInt})", sleep as i64)
                        .execute(conn).await?;
                }

                use std::collections::HashSet;

                let mut delete_set = HashSet::new();

                // Condition 1: Predecessors that already marked their predecessors
                for row in &predecessors {
                    if let (Some(pred_object_id), Some(pred_cp_sequence_number), Some(true)) =
                        (&row.pred_object_id, &row.pred_cp_sequence_number, &row.pred_marked_predecessor) {
                        delete_set.insert((pred_object_id.clone(), *pred_cp_sequence_number));
                    }
                }

                // Condition 2: Intermediate entries (latest rows that appear as predecessors)
                for p1 in &predecessors {
                    for p2 in &predecessors {
                        if let (Some(p2_pred_id), Some(p2_pred_seq)) = (&p2.pred_object_id, &p2.pred_cp_sequence_number) {
                            if p1.latest_object_id == *p2_pred_id && p1.latest_cp_sequence_number == *p2_pred_seq {
                                delete_set.insert((p1.latest_object_id.clone(), p1.latest_cp_sequence_number));
                            }
                        }
                    }
                }

                let values = delete_set
                    .iter()
                    .map(|(object_id, cp_sequence_number)| {
                        let object_id_hex = hex::encode(object_id);
                        format!("('\\x{}'::BYTEA, {}::BIGINT)", object_id_hex, cp_sequence_number)
                    })
                    .collect::<Vec<_>>()
                    .join(",");

                let pred_deleted_count = if values.is_empty() {
                    0
                } else {
                    let query = format!(
                        "
                        DELETE FROM obj_info
                        WHERE (object_id, cp_sequence_number) IN (
                            VALUES {}
                        )
                        ",
                        values
                    );
                    sql_query(query).execute(conn).await? as i64
                };

                // Step 3: Mark remaining predecessors as obsolete
                if let Some(sleep) = pred_obsolete_sleep {
                    postgres::sql_query!("SELECT pg_sleep({BigInt})", sleep as i64)
                        .execute(conn).await?;
                }

                let values = predecessors
                .iter()
                .filter_map(|row| {
                    // Only include rows where predecessor exists and hasn't marked its predecessor
                    if let (Some(pred_object_id), Some(pred_cp_sequence_number), Some(false)) =
                        (&row.pred_object_id, &row.pred_cp_sequence_number, &row.pred_marked_predecessor) {
                        let object_id_hex = hex::encode(pred_object_id);
                        Some(format!("('\\x{}'::BYTEA, {}::BIGINT, {}::BIGINT)",
                                   object_id_hex, pred_cp_sequence_number, row.latest_cp_sequence_number))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(",");

            // Handle empty case
            let pred_obsolete_count = if values.is_empty() {
                0
            } else {
                let query = format!(
                    "
                    UPDATE obj_info
                    SET obsolete_at = v.latest_cp_sequence_number
                    FROM (VALUES {}) AS v(object_id, cp_sequence_number, latest_cp_sequence_number)
                    WHERE obj_info.object_id = v.object_id
                      AND obj_info.cp_sequence_number = v.cp_sequence_number
                    ",
                    values
                );
                sql_query(query).execute(conn).await? as i64
            };

                // Step 4: Mark latest rows as having marked their predecessors
                if let Some(sleep) = marked_predecessor_sleep {
                    postgres::sql_query!("SELECT pg_sleep({BigInt})", sleep as i64)
                        .execute(conn).await?;
                }

                let values = predecessors
                .iter()
                .map(|row| {
                    let object_id_hex = hex::encode(&row.latest_object_id);
                    format!("('\\x{}'::BYTEA, {}::BIGINT)", object_id_hex, row.latest_cp_sequence_number)
                })
                .collect::<Vec<_>>()
                .join(",");

            let marked_predecessor_count = if values.is_empty() {
                0
            } else {
                let query = format!(
                    "
                    UPDATE obj_info
                    SET marked_predecessor = true
                    WHERE (object_id, cp_sequence_number) IN (
                        VALUES {}
                    )
                    ",
                    values
                );
                sql_query(query).execute(conn).await? as i64
            };

                let end = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

                Ok(T0Result {
                    pred_deleted: pred_deleted_count as i64,
                    pred_obsolete: pred_obsolete_count as i64,
                    marked_predecessor: marked_predecessor_count as i64,
                    start_timestamp_ms: start,
                    end_timestamp_ms: end,
                })
            })
        }).await?;

        Ok(result)
    }

    async fn t1(conn: &mut Connection<'_>, from: u64, to_exclusive: u64) -> Result<T1Result> {
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let query = postgres::sql_query!(
            "
            WITH start_timing AS (
            SELECT (extract(epoch from clock_timestamp()) * 1000)::BIGINT as start_ts
        ),
     deletion_latest AS (
        DELETE FROM obj_info
        WHERE cp_sequence_number >= {BigInt}
          AND cp_sequence_number < {BigInt}
          AND marked_predecessor = true
          AND obsolete_at IS NOT NULL
        RETURNING object_id
    ),
    deletion_obsolete AS (
        DELETE FROM obj_info
        WHERE obsolete_at >= {BigInt}
          AND obsolete_at < {BigInt}
          AND marked_predecessor = true
        RETURNING object_id
    )
    SELECT
        (SELECT COUNT(*)::BIGINT FROM deletion_latest) as deleted_latest_count,
        (SELECT COUNT(*)::BIGINT FROM deletion_obsolete) as deleted_obsolete_count,
        (SELECT start_ts FROM start_timing) as start_timestamp_ms,
        (SELECT extract(epoch from clock_timestamp()) * 1000)::BIGINT as end_timestamp_ms;
    ",
            from as i64,
            to_exclusive as i64,
            from as i64,
            to_exclusive as i64,
        );

        let mut result: T1Result = query.get_result(conn).await?;

        let end = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        result.end_timestamp_ms = end;
        result.start_timestamp_ms = start;

        Ok(result)
    }

    #[tokio::test]
    async fn test_process_basics() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint1)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 0);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(0, 1, &mut conn).await.unwrap();
        // The object is newly created, so no prior state to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        let addr0 = TestCheckpointDataBuilder::derive_address(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Address));
        assert_eq!(all_obj_info[0].owner_id, Some(addr0.to_vec()));

        builder = builder
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint2)).unwrap();
        assert!(result.is_empty());
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 0);
        let rows_pruned = obj_info.prune(1, 2, &mut conn).await.unwrap();
        // No new entries are inserted to the table, so no old entries to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint3)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 2);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let addr1 = TestCheckpointDataBuilder::derive_address(1);
        assert_eq!(all_obj_info.len(), 2);
        assert_eq!(all_obj_info[1].object_id, object0.to_vec());
        assert_eq!(all_obj_info[1].cp_sequence_number, 2);
        assert_eq!(all_obj_info[1].owner_kind, Some(StoredOwnerKind::Address));
        assert_eq!(all_obj_info[1].owner_id, Some(addr1.to_vec()));

        let rows_pruned = obj_info.prune(2, 3, &mut conn).await.unwrap();
        // The object is transferred, so we prune the old entry.
        assert_eq!(rows_pruned, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        // Only the new entry is left in the table.
        assert_eq!(all_obj_info[0].cp_sequence_number, 2);

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint4 = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint4)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert_eq!(processed.cp_sequence_number, 3);
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(3, 4, &mut conn).await.unwrap();
        // The object is deleted, so we prune both the old entry and the delete entry.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);
    }

    #[tokio::test]
    async fn test_process_noop() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        // In this checkpoint, an object is created and deleted in the same checkpoint.
        // We expect that no updates are made to the table.
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert!(result.is_empty());
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        let rows_pruned = obj_info.prune(0, 1, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 0);
    }

    #[tokio::test]
    async fn test_process_wrap() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Delete(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        assert_eq!(all_obj_info.len(), 2);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);
        assert!(all_obj_info[0].owner_kind.is_some());
        assert_eq!(all_obj_info[1].object_id, object0.to_vec());
        assert_eq!(all_obj_info[1].cp_sequence_number, 1);
        assert!(all_obj_info[1].owner_kind.is_none());

        let rows_pruned = obj_info.prune(0, 2, &mut conn).await.unwrap();
        // Both the creation entry and the wrap entry will be pruned.
        assert_eq!(rows_pruned, 2);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);

        builder = builder
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(2, 3, &mut conn).await.unwrap();
        // No entry prior to this checkpoint, so no entries to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 2);
        assert!(all_obj_info[0].owner_kind.is_some());
    }

    #[tokio::test]
    async fn test_process_shared_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_shared_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(0, 1, &mut conn).await.unwrap();
        // No entry prior to this checkpoint, so no entries to prune.
        assert_eq!(rows_pruned, 0);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 0);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Shared));
    }

    #[tokio::test]
    async fn test_process_immutable_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::Immutable)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);
        let rows_pruned = obj_info.prune(0, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned.
        assert_eq!(rows_pruned, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 1);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Immutable));
    }

    #[tokio::test]
    async fn test_process_object_owned_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .change_object_owner(0, Owner::ObjectOwner(dbg_addr(0)))
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let rows_pruned = obj_info.prune(1, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned.
        assert_eq!(rows_pruned, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        let addr0 = TestCheckpointDataBuilder::derive_address(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 1);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Object));
        assert_eq!(all_obj_info[0].owner_id, Some(addr0.to_vec()));
    }

    #[tokio::test]
    async fn test_process_consensus_v2_object() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        builder = builder
            .start_transaction(0)
            .change_object_owner(
                0,
                Owner::ConsensusAddressOwner {
                    start_version: SequenceNumber::from_u64(1),
                    owner: dbg_addr(0),
                },
            )
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert_eq!(result.len(), 1);
        let processed = &result[0];
        assert!(matches!(
            processed.update,
            ProcessedObjInfoUpdate::Insert(_)
        ));
        let rows_inserted = ObjInfo::commit(&result, &mut conn).await.unwrap();
        assert_eq!(rows_inserted, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 2);

        let rows_pruned = obj_info.prune(1, 2, &mut conn).await.unwrap();
        // The creation entry will be pruned.
        assert_eq!(rows_pruned, 1);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let object0 = TestCheckpointDataBuilder::derive_object_id(0);
        let addr0 = TestCheckpointDataBuilder::derive_address(0);
        assert_eq!(all_obj_info.len(), 1);
        assert_eq!(all_obj_info[0].object_id, object0.to_vec());
        assert_eq!(all_obj_info[0].cp_sequence_number, 1);
        assert_eq!(all_obj_info[0].owner_kind, Some(StoredOwnerKind::Address));
        assert_eq!(all_obj_info[0].owner_id, Some(addr0.to_vec()));
    }

    #[tokio::test]
    async fn test_obj_info_batch_prune() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        let rows_pruned = obj_info.prune(0, 3, &mut conn).await.unwrap();
        assert_eq!(rows_pruned, 3);

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        assert_eq!(all_obj_info.len(), 0);
    }

    #[tokio::test]
    async fn test_obj_info_prune_with_missing_data() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Cannot prune checkpoint 1 yet since we haven't processed the checkpoint 1 data.
        // This should not yet remove the prune info for checkpoint 0.
        assert!(obj_info.prune(0, 2, &mut conn).await.is_err());

        builder = builder
            .start_transaction(0)
            .transfer_object(0, 1)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune both checkpoints 0 and 1.
        obj_info.prune(0, 2, &mut conn).await.unwrap();

        builder = builder
            .start_transaction(1)
            .transfer_object(0, 0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Checkpoint 3 is missing, so we can not prune it.
        assert!(obj_info.prune(2, 4, &mut conn).await.is_err());

        builder = builder
            .start_transaction(2)
            .delete_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let values = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&values, &mut conn).await.unwrap();

        // Now we can prune checkpoint 2, as well as 3.
        obj_info.prune(2, 4, &mut conn).await.unwrap();
    }

    /// In our processing logic, we consider objects that appear as input to the checkpoint but not
    /// in the output as wrapped or deleted. This emits a tombstone row. Meanwhile, the remote store
    /// containing `CheckpointData` used to include unchanged shared objects in the `input_objects`
    /// of a `CheckpointTransaction`. Because these read-only shared objects were not modified, they
    ///were not included in `output_objects`. But that means within our pipeline, these object
    /// states were incorrectly treated as deleted, and thus every transaction read emitted a
    /// tombstone row. This test validates that unless an object appears as an input object from
    /// `tx.effects.object_changes`, we do not consider it within our pipeline.
    ///
    /// Use the checkpoint builder to create a shared object. Then, remove this from the checkpoint,
    /// and replace it with a transaction that takes the shared object as read-only.
    #[tokio::test]
    async fn test_process_unchanged_shared_object() {
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0)
            .start_transaction(0)
            .create_shared_object(1)
            .finish_transaction();

        builder.build_checkpoint();

        builder = builder
            .start_transaction(0)
            .read_shared_object(1)
            .finish_transaction();

        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_concurrent_cit0_cjt0_cit0_first() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);

        let repeats = 4;
        let range = 1000;

        // Create range * repeats objects, ie 4x1000 = 4000
        for r in 0..repeats {
            for i in 0..range {
                builder = builder
                    .start_transaction(0)
                    .create_owned_object(i + r * range)
                    .finish_transaction();
            }
            let checkpoint = builder.build_checkpoint();
            let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
            ObjInfo::commit(&result, &mut conn).await.unwrap();
        }

        // Modifies all objects, ie 4x1000 = 4000
        for r in 0..repeats {
            for i in 0..range {
                builder = builder
                    .start_transaction(0)
                    .transfer_object(i + r * range, 1)
                    .finish_transaction();
            }
            let checkpoint = builder.build_checkpoint();
            let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
            ObjInfo::commit(&result, &mut conn).await.unwrap();
        }

        // Modifies all objects again
        for r in 0..repeats {
            for i in 0..range {
                builder = builder
                    .start_transaction(1)
                    .transfer_object(i + r * range, 0)
                    .finish_transaction();
            }
            let checkpoint = builder.build_checkpoint();
            let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
            ObjInfo::commit(&result, &mut conn).await.unwrap();
        }

        let total_checkpoints = repeats * 3;
        let cit0_end_exclusive = repeats * 2;

        // Run cit0 and cjt0 concurrently, giving cjt0 additional sleep so that it'll terminate after cit0

        let db = indexer.store().clone();

        let ci = tokio::spawn(async move {
            let mut conn1 = db.connect().await.unwrap();
            let cit0_sleep = 0;
            let t0_result = t0(
                &mut conn1,
                0,
                cit0_end_exclusive,
                Some(cit0_sleep),
                Some(cit0_sleep),
                Some(cit0_sleep),
                Some(cit0_sleep),
                "CiT0".to_string(),
            )
            .await
            .unwrap();
            let t1_result = t1(&mut conn1, 0, cit0_end_exclusive).await.unwrap();
            (t0_result, t1_result)
        });

        let db = indexer.store().clone();
        let cj = tokio::spawn(async move {
            let mut conn2 = db.connect().await.unwrap();
            let cjt0_sleep = 0;
            let t0_result = t0(
                &mut conn2,
                cit0_end_exclusive,
                total_checkpoints,
                Some(cjt0_sleep),
                Some(cjt0_sleep),
                Some(cjt0_sleep),
                Some(cjt0_sleep),
                "CjT0".to_string(),
            )
            .await
            .unwrap();
            let t1_result = t1(&mut conn2, cit0_end_exclusive, total_checkpoints)
                .await
                .unwrap();
            (t0_result, t1_result)
        });

        let (ci_result, cj_result) = tokio::join!(ci, cj);
        match (&ci_result, &cj_result) {
            (Ok(_), Ok(_)) => {
                println!("Case: Both transactions completed successfully");
            }
            _ => {
                println!("Deadlock encountered");
                return;
            }
        }

        let ci_result = ci_result.unwrap();
        let cj_result = cj_result.unwrap();

        // Create timeline diagram
        let mut timeline = TimelineDiagram::new();
        timeline.add_transaction_events(&ci_result.0, "CiT0");
        timeline.add_t1_events(&ci_result.1, "CiT1");
        timeline.add_transaction_events(&cj_result.0, "CjT0");
        timeline.add_t1_events(&cj_result.1, "CjT1");

        println!("{}", timeline.generate_timeline());

        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let mut orphaned = 0;
        let mut obsoleted = 0;
        let mut marked_predecessor = 0;

        for obj in all_obj_info {
            // println!("object_id: {:?}, cp_sequence_number: {}, obsolete_at: {:?}, marked_predecessor: {}",
            //     obj.object_id, obj.cp_sequence_number, obj.obsolete_at, obj.marked_predecessor
            // );
            if obj.marked_predecessor && obj.obsolete_at.is_some() {
                orphaned += 1;
            }
            if obj.obsolete_at.is_some() {
                obsoleted += 1;
            }
            if obj.marked_predecessor {
                marked_predecessor += 1;
            }
        }
        println!(
            "orphaned: {}, obsoleted: {}, marked_predecessor: {}",
            orphaned, obsoleted, marked_predecessor
        );
    }

    #[tokio::test]
    async fn test_deadlock_single_object_guaranteed() {
        let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();
        let obj_info = ObjInfo::default();
        let mut builder = TestCheckpointDataBuilder::new(0);

        // Create one object and modify it across many checkpoints
        // This guarantees maximum overlap and lock contention

        // Checkpoint 0: Create object 0
        builder = builder
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint = builder.build_checkpoint();
        let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
        ObjInfo::commit(&result, &mut conn).await.unwrap();

        let checkpoints = 10;

        // Checkpoints 1-10: Keep modifying the same object
        for i in 1..=10 {
            builder = builder
                .start_transaction(0)
                .transfer_object(0, i % 2) // Alternate between addresses 0 and 1
                .finish_transaction();
            let checkpoint = builder.build_checkpoint();
            let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
            ObjInfo::commit(&result, &mut conn).await.unwrap();
        }

        println!("Created 11 checkpoints all modifying the same object");
        let db1 = indexer.store().clone();
        let db2 = indexer.store().clone();

        let ci_range = (0, 6);
        let cj_range = (6, 13);

        println!("Ci will process checkpoints {}..{}", ci_range.0, ci_range.1);
        println!("Cj will process checkpoints {}..{}", cj_range.0, cj_range.1);

        let ci = tokio::spawn(async move {
            let mut conn1 = db1.connect().await.unwrap();
            let cit0_sleep = 0;
            let t0_result = t0(
                &mut conn1,
                ci_range.0,
                ci_range.1,
                Some(cit0_sleep),
                Some(cit0_sleep),
                Some(cit0_sleep),
                Some(cit0_sleep),
                "CiT0".to_string(),
            )
            .await
            .unwrap();
            let t1_result = t1(&mut conn1, ci_range.0, ci_range.1).await.unwrap();
            (t0_result, t1_result)
        });

        let cj = tokio::spawn(async move {
            let mut conn2 = db2.connect().await.unwrap();
            let cjt0_sleep = 0;
            let t0_result = t0(
                &mut conn2,
                cj_range.0,
                cj_range.1,
                Some(cjt0_sleep),
                Some(cjt0_sleep),
                Some(cjt0_sleep),
                Some(cjt0_sleep),
                "CjT0".to_string(),
            )
            .await
            .unwrap();
            let t1_result = t1(&mut conn2, cj_range.0, cj_range.1).await.unwrap();
            (t0_result, t1_result)
        });

        let (ci_result, cj_result) = tokio::join!(ci, cj);
        match (&ci_result, &cj_result) {
            (Ok(_), Ok(_)) => {
                println!("Case: Both transactions completed successfully");
            }
            _ => {
                println!("Deadlock encountered");
                return;
            }
        }

        let mut timeline = TimelineDiagram::new();
        let ci_result = ci_result.unwrap();
        let cj_result = cj_result.unwrap();
        timeline.add_transaction_events(&ci_result.0, "CiT0");
        timeline.add_t1_events(&ci_result.1, "CiT1");
        timeline.add_transaction_events(&cj_result.0, "CjT0");
        timeline.add_t1_events(&cj_result.1, "CjT1");
        println!("{}", timeline.generate_timeline());

        // Check final state regardless
        let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
        let same_object_entries: Vec<_> = all_obj_info
            .iter()
            .filter(|obj| obj.object_id == TestCheckpointDataBuilder::derive_object_id(0).to_vec())
            .collect();

        println!("Entries for object 0: {}", same_object_entries.len());
        for entry in same_object_entries {
            println!(
                "  cp: {}, obsolete_at: {:?}, marked_predecessor: {}",
                entry.cp_sequence_number, entry.obsolete_at, entry.marked_predecessor
            );
        }
    }
}
