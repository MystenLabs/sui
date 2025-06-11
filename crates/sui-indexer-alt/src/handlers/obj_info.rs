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
    use std::collections::{BTreeMap, BTreeSet};

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
        intermediate_deleted: i64,
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

    #[derive(Debug, Clone)]
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
                    "pred_deleted: {}, intermediate_deleted: {}, pred_obsolete: {}, marked_predecessor: {}",
                    result.pred_deleted, result.intermediate_deleted, result.pred_obsolete, result.marked_predecessor
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
            use std::collections::HashMap;

            // Extract all T0 transactions and their timing
            let mut t0_transactions: HashMap<String, (i64, i64)> = HashMap::new();

            for event in &self.events {
                if event.label.contains("T0") {
                    if let Some(worker_part) = event.label.split("T0").next() {
                        let entry = t0_transactions
                            .entry(worker_part.to_string())
                            .or_insert((0, 0));
                        if event.label.ends_with("_START") {
                            entry.0 = event.timestamp_ms;
                        } else if event.label.ends_with("_END") {
                            entry.1 = event.timestamp_ms;
                        }
                    }
                }
            }

            let events: Vec<&str> = self.events.iter().map(|e| e.label.as_str()).collect();
            let pattern = events.join(" → ");

            if t0_transactions.len() <= 1 {
                format!("SINGLE WORKER [{}]", pattern)
            } else {
                // Check if any T0 transactions overlap
                let mut overlapping = false;
                let transactions: Vec<_> = t0_transactions.values().collect();

                for i in 0..transactions.len() {
                    for j in (i + 1)..transactions.len() {
                        let (start1, end1) = transactions[i];
                        let (start2, end2) = transactions[j];

                        // Check if they overlap
                        if start1.max(start2) < end1.min(end2) {
                            overlapping = true;
                            break;
                        }
                    }
                    if overlapping {
                        break;
                    }
                }

                if overlapping {
                    format!("CONCURRENT [{}]", pattern)
                } else {
                    format!("SEQUENTIAL [{}]", pattern)
                }
            }
        }

        fn analyze_overlaps(&self) -> String {
            use std::collections::HashMap;

            // Group events by worker and transaction type
            let mut workers: HashMap<String, (Option<&TimelineEvent>, Option<&TimelineEvent>)> =
                HashMap::new();

            for event in &self.events {
                if event.label.contains("T0") {
                    // Extract worker ID from labels like "C0T0_START", "C1T0_END", etc.
                    if let Some(worker_part) = event.label.split("T0").next() {
                        let entry = workers
                            .entry(worker_part.to_string())
                            .or_insert((None, None));
                        if event.label.ends_with("_START") {
                            entry.0 = Some(event);
                        } else if event.label.ends_with("_END") {
                            entry.1 = Some(event);
                        }
                    }
                }
            }

            let worker_count = workers.len();

            if worker_count == 0 {
                return "No T0 transactions found".to_string();
            }

            if worker_count == 1 {
                // Single worker case
                let (worker_name, (start, end)) = workers.iter().next().unwrap();
                match (start, end) {
                    (Some(start), Some(end)) => {
                        let duration = end.timestamp_ms - start.timestamp_ms;
                        format!("Single worker {}: {}ms duration", worker_name, duration)
                    }
                    _ => "Incomplete single worker timing data".to_string(),
                }
            } else {
                // Multiple workers case - analyze overlaps
                let mut worker_data: Vec<(String, i64, i64, i64)> = Vec::new();

                for (worker_name, (start, end)) in &workers {
                    if let (Some(start), Some(end)) = (start, end) {
                        let duration = end.timestamp_ms - start.timestamp_ms;
                        worker_data.push((
                            worker_name.clone(),
                            start.timestamp_ms,
                            end.timestamp_ms,
                            duration,
                        ));
                    }
                }

                if worker_data.len() < 2 {
                    return "Insufficient complete worker timing data for overlap analysis"
                        .to_string();
                }

                // Sort by start time for cleaner analysis
                worker_data.sort_by_key(|(_, start_time, _, _)| *start_time);

                let mut result = String::new();

                // Check each pair of workers for overlap
                for i in 0..worker_data.len() {
                    for j in (i + 1)..worker_data.len() {
                        let (name1, start1, end1, _dur1) = &worker_data[i];
                        let (name2, start2, end2, _dur2) = &worker_data[j];

                        let overlap_start = start1.max(start2);
                        let overlap_end = end1.min(end2);

                        if overlap_start < overlap_end {
                            let overlap_duration = overlap_end - overlap_start;
                            if !result.is_empty() {
                                result.push_str(", ");
                            }
                            result
                                .push_str(&format!("{}↔{}: {}ms", name1, name2, overlap_duration));
                        }
                    }
                }

                if result.is_empty() {
                    // No overlaps found - show sequential execution
                    let worker_summaries: Vec<String> = worker_data
                        .iter()
                        .map(|(name, _, _, duration)| format!("{}: {}ms", name, duration))
                        .collect();
                    format!("Sequential execution ({})", worker_summaries.join(", "))
                } else {
                    // Show overlaps
                    let worker_summaries: Vec<String> = worker_data
                        .iter()
                        .map(|(name, _, _, duration)| format!("{}: {}ms", name, duration))
                        .collect();
                    format!(
                        "Overlaps: {} | Workers: ({})",
                        result,
                        worker_summaries.join(", ")
                    )
                }
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
            ),
            pred_deleted_sleep AS (
                SELECT pg_sleep({BigInt}) as dummy
            ),
            -- Otherwise, flag them for later deletion
            pred_obsolete AS (
                UPDATE obj_info
                SET obsolete_at = p.latest_cp_sequence_number
                FROM predecessors p
                WHERE obj_info.object_id = p.pred_object_id
                  AND obj_info.cp_sequence_number = p.pred_cp_sequence_number
                  AND p.pred_marked_predecessor = false
                RETURNING obj_info.object_id
            ),
            pred_obsolete_sleep AS (
                SELECT pg_sleep({BigInt}) as dummy
            ),
            -- Finally, mark 'latest' rows to have marked their own immediate predecessors
            marked_predecessor AS (
                UPDATE obj_info
                SET marked_predecessor = true
                WHERE (object_id, cp_sequence_number) IN (
                    SELECT latest_object_id, latest_cp_sequence_number FROM predecessors
                    ORDER BY latest_object_id, latest_cp_sequence_number
                )
                RETURNING object_id
            ),
            marked_predecessor_sleep AS (
                SELECT pg_sleep({BigInt}) as dummy
            ),
            -- Delete preds that already marked their own immediate predecessors
            pred_deleted AS (
                DELETE FROM obj_info
                WHERE (object_id, cp_sequence_number) IN (
                    SELECT pred_object_id, pred_cp_sequence_number
                    FROM predecessors
                    WHERE pred_marked_predecessor = true
                )
                RETURNING object_id
            ),
            -- Delete intermediate entries among 'latest' changes
            intermediate_deleted AS (
                DELETE FROM obj_info
                WHERE (object_id, cp_sequence_number) IN (
                    SELECT latest_object_id, latest_cp_sequence_number
                    FROM predecessors p1
                    WHERE EXISTS (
                        SELECT 1 FROM predecessors p2
                        WHERE p2.pred_object_id = p1.latest_object_id
                        AND p2.pred_cp_sequence_number = p1.latest_cp_sequence_number
                    )
                )
                RETURNING object_id
            )
            SELECT
                (SELECT COUNT(*)::BIGINT FROM pred_deleted) as pred_deleted,
                (SELECT COUNT(*)::BIGINT FROM intermediate_deleted) as intermediate_deleted,
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

        let repeats = 40;
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

        let checkpoints: u64 = 1000;

        // Checkpoints 1-10: Keep modifying the same object
        for i in 1..=checkpoints {
            builder = builder
                .start_transaction(0)
                .transfer_object(0, (i % 2) as u8) // Alternate between addresses 0 and 1
                .finish_transaction();
            let checkpoint = builder.build_checkpoint();
            let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
            ObjInfo::commit(&result, &mut conn).await.unwrap();
        }

        println!("Created 11 checkpoints all modifying the same object");
        let db1 = indexer.store().clone();
        let db2 = indexer.store().clone();

        let ci_range = (0, checkpoints / 2);
        let cj_range = (checkpoints / 2, checkpoints + 1);

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

    #[tokio::test]
    async fn test_parameterized_pruning_stress() {
        // Helper function to run the test with different configurations
        async fn run_pruning_test(
            objects: u64,
            modification_rounds: u64,
            workers: u64,
            sleep_multiplier: u64,
        ) -> bool {
            let (indexer, _db) = Indexer::new_for_testing(&MIGRATIONS).await;
            let mut conn = indexer.store().connect().await.unwrap();
            let obj_info = ObjInfo::default();
            let mut builder = TestCheckpointDataBuilder::new(0);

            println!(
                "  Setting up {} objects with {} modification rounds...",
                objects, modification_rounds
            );

            // Setup phase
            for i in 0..objects {
                builder = builder
                    .start_transaction(0)
                    .create_owned_object(i)
                    .finish_transaction();
                let checkpoint = builder.build_checkpoint();
                let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
                ObjInfo::commit(&result, &mut conn).await.unwrap();
            }

            println!(
                "{} checkpoints created, one object created per checkpoint",
                objects
            );

            for round in 0..modification_rounds {
                for i in 0..objects {
                    builder = builder
                        .start_transaction(0)
                        .transfer_object(i, (round % 3) as u8)
                        .finish_transaction();
                    let checkpoint = builder.build_checkpoint();
                    let result = obj_info.process(&Arc::new(checkpoint)).unwrap();
                    ObjInfo::commit(&result, &mut conn).await.unwrap();
                }
            }

            println!(
                "{} checkpoints created, one object modified per checkpoint",
                objects * modification_rounds,
            );

            let total_checkpoints = objects + (objects * modification_rounds);
            // drop the - objects if we want to process all checkpoints
            // let pruning_end = total_checkpoints - objects;
            let pruning_end = total_checkpoints;
            let work_per_worker = pruning_end / workers;

            println!(
                "  Spawning {} workers to prune checkpoints 0..{}",
                workers, pruning_end
            );

            // Create worker ranges and spawn tasks
            let mut handles = Vec::new();
            let mut prune_ranges = Vec::new();
            for i in 0..workers {
                let start = i * work_per_worker;
                let end = if i == workers - 1 {
                    pruning_end
                } else {
                    (i + 1) * work_per_worker
                };
                prune_ranges.push((start, end));

                println!("    Worker {}: range {}..{}", i, start, end);

                let db = indexer.store().clone();
                let handle = tokio::spawn(async move {
                    let mut conn = db.connect().await.unwrap();
                    let _sleep_base = (i * sleep_multiplier) as u64;

                    let t0_result = t0(
                        &mut conn,
                        start,
                        end,
                        // Some(sleep_base + 100),
                        // Some(sleep_base + 25),
                        // Some(sleep_base + 75),
                        // Some(sleep_base + 50),
                        None,
                        None,
                        None,
                        None,
                        format!("C{}T0", i),
                    )
                    .await?;

                    let t1_result = t1(&mut conn, start, end).await?;
                    Ok::<(T0Result, T1Result), anyhow::Error>((t0_result, t1_result))
                });

                handles.push(handle);
            }

            // Wait for all workers and collect results
            let results = futures::future::join_all(handles).await;
            let mut successful_workers = Vec::new();
            let mut failed = false;

            for (worker_id, result) in results.into_iter().enumerate() {
                match result {
                    Ok(Ok((t0_result, t1_result))) => {
                        successful_workers.push((worker_id, t0_result, t1_result));
                    }
                    Ok(Err(e)) => {
                        println!("    Worker {} failed with error: {:?}", worker_id, e);
                        failed = true;
                    }
                    Err(e) => {
                        println!("    Worker {} task failed: {:?}", worker_id, e);
                        failed = true;
                    }
                }
            }

            if failed {
                println!("  DEADLOCK DETECTED in this configuration");
                return false;
            }

            // Generate timeline diagram for successful runs
            println!("  All workers completed successfully, generating timeline...");
            let mut timeline = TimelineDiagram::new();
            for (worker_id, t0_result, t1_result) in &successful_workers {
                timeline.add_transaction_events(t0_result, &format!("C{}T0", worker_id));
                timeline.add_t1_events(t1_result, &format!("C{}T1", worker_id));
            }

            println!("{}", timeline.generate_timeline());

            println!("=== STARTING VALIDATION ===");
            let all_obj_info = get_all_obj_info(&mut conn).await.unwrap();
            let total_entries = all_obj_info.len();

            let validation_result = validate_obj_info_table(&mut conn, prune_ranges)
                .await
                .unwrap();

            // Print validation summary
            println!("=== VALIDATION SUMMARY ===");
            println!("Total entries: {}", validation_result.stats.total_entries);
            println!("Total objects: {}", validation_result.stats.total_objects);
            println!(
                "Orphaned entries: {}",
                validation_result.stats.orphaned_entries
            );
            println!(
                "Obsolete entries: {}",
                validation_result.stats.obsolete_entries
            );
            println!(
                "Marked predecessor entries: {}",
                validation_result.stats.marked_predecessor_entries
            );
            println!(
                "Objects with multiple entries: {}",
                validation_result.stats.objects_with_multiple_entries
            );
            println!(
                "Max entries per object: {}",
                validation_result.stats.max_entries_per_object
            );

            let avg_entries_per_object = if validation_result.stats.total_objects > 0 {
                validation_result.stats.total_entries as f64
                    / validation_result.stats.total_objects as f64
            } else {
                0.0
            };
            println!("Average entries per object: {:.2}", avg_entries_per_object);

            if validation_result.valid {
                println!("✅ All validations passed!");
            } else {
                println!(
                    "❌ Found {} validation errors:",
                    validation_result.errors.len()
                );
                for error in &validation_result.errors {
                    println!("  - {}", error);
                }
                return false; // Validation failed
            }

            // Analyze final state
            let mut orphaned = 0;
            let mut obsoleted = 0;
            let mut marked_predecessor = 0;

            for obj in all_obj_info {
                // println!(
                // "object_id: {:?}, cp_sequence_number: {}, obsolete_at: {:?}, marked_predecessor: {}",
                // obj.object_id,
                // obj.cp_sequence_number,
                // obj.obsolete_at,
                // obj.marked_predecessor,
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
                "  Final state: {} total entries, {} orphaned, {} obsoleted, {} marked",
                total_entries, orphaned, obsoleted, marked_predecessor
            );

            if orphaned > 0 {
                println!(
                    "  WARNING: Found {} orphaned entries - potential correctness issue!",
                    orphaned
                );
            }

            true // All workers succeeded
        }

        // Test different configurations
        let configs = vec![
            // (100, 3, 2, 50),   // Small: 100 objects, 3 rounds, 2 workers
            // (500, 4, 3, 75),   // Medium: 500 objects, 4 rounds, 3 workers
            // (1000, 5, 4, 100), // Large: 1000 objects, 5 rounds, 4 workers
            // (200, 10, 5, 25),  // High contention: fewer objects, more rounds, more workers
            (10000, 20, 20, 0),
        ];

        for (objects, rounds, workers, sleep_mult) in configs {
            println!(
                "Testing config: {} objects, {} rounds, {} workers, {}ms sleep multiplier",
                objects, rounds, workers, sleep_mult
            );

            let success = run_pruning_test(objects, rounds, workers, sleep_mult).await;
            println!("Result: {}", if success { "SUCCESS" } else { "DEADLOCK" });
        }
    }

    #[derive(Debug)]
    struct ValidationResult {
        pub valid: bool,
        pub errors: Vec<String>,
        pub stats: ValidationStats,
    }

    #[derive(Debug)]
    struct ValidationStats {
        pub total_entries: usize,
        pub total_objects: usize,
        pub orphaned_entries: usize,
        pub obsolete_entries: usize,
        pub marked_predecessor_entries: usize,
        pub objects_with_multiple_entries: usize,
        pub max_entries_per_object: usize,
    }

    async fn validate_obj_info_table(
        conn: &mut Connection<'_>,
        pruned_ranges: Vec<(u64, u64)>, // (from, to_exclusive) ranges that were pruned
    ) -> Result<ValidationResult> {
        let all_obj_info = get_all_obj_info(conn).await?;
        let mut errors = Vec::new();

        // Group entries by object_id
        let mut object_entries: BTreeMap<Vec<u8>, Vec<&StoredObjInfo>> = BTreeMap::new();
        for entry in &all_obj_info {
            object_entries
                .entry(entry.object_id.clone())
                .or_insert_with(Vec::new)
                .push(entry);
        }

        // Sort entries for each object by checkpoint sequence number
        for entries in object_entries.values_mut() {
            entries.sort_by_key(|e| e.cp_sequence_number);
        }

        let mut stats = ValidationStats {
            total_entries: all_obj_info.len(),
            total_objects: object_entries.len(),
            orphaned_entries: 0,
            obsolete_entries: 0,
            marked_predecessor_entries: 0,
            objects_with_multiple_entries: 0,
            max_entries_per_object: 0,
        };

        // 2. Per-object validation
        for (object_id, entries) in &object_entries {
            let object_id = ObjectID::from_bytes(object_id).unwrap();

            stats.max_entries_per_object = stats.max_entries_per_object.max(entries.len());
            if entries.len() > 1 {
                stats.objects_with_multiple_entries += 1;
            }

            // Validate checkpoint sequence ordering
            for window in entries.windows(2) {
                if window[0].cp_sequence_number >= window[1].cp_sequence_number {
                    errors.push(format!(
                        "Object {}: Entries not in checkpoint order: {} >= {}",
                        object_id, window[0].cp_sequence_number, window[1].cp_sequence_number
                    ));
                }
            }

            // Validate marked_predecessor logic and count stats
            for entry in entries.iter() {
                if entry.marked_predecessor {
                    stats.marked_predecessor_entries += 1;
                }

                if entry.obsolete_at.is_some() {
                    stats.obsolete_entries += 1;

                    // If obsolete_at is set, there should be a newer entry for this object
                    let has_newer_entry = entries
                        .iter()
                        .any(|e| e.cp_sequence_number > entry.cp_sequence_number);

                    if !has_newer_entry {
                        errors.push(format!(
                        "Object {}: Entry at cp {} is marked obsolete but no newer entry exists",
                        object_id, entry.cp_sequence_number
                    ));
                    }
                }

                // Check for orphaned entries
                if entry.marked_predecessor && entry.obsolete_at.is_some() {
                    stats.orphaned_entries += 1;
                    errors.push(format!(
                    "Object {}: Entry at cp {} is orphaned (marked_predecessor=true AND obsolete_at=Some)",
                    object_id, entry.cp_sequence_number
                ));
                }
            }

            // Validate pruning effectiveness
            for &(prune_from, prune_to) in &pruned_ranges {
                let entries_in_range: Vec<_> = entries
                    .iter()
                    .filter(|e| {
                        e.cp_sequence_number >= prune_from as i64
                            && e.cp_sequence_number < prune_to as i64
                    })
                    .collect();

                // After pruning, each object should have at most 2 entries per pruned range:
                // 1. First appearance (boundary anchor)
                // 2. Last appearance (if it's the latest state)
                if entries_in_range.len() > 2 {
                    errors.push(format!(
                        "Object {}: Too many entries ({}) in pruned range {}-{}, expected ≤ 2",
                        object_id,
                        entries_in_range.len(),
                        prune_from,
                        prune_to
                    ));
                }

                // If there are 2 entries, first should be marked_predecessor=true, last should be false
                if entries_in_range.len() == 2 {
                    let first = entries_in_range[0];
                    let last = entries_in_range[1];

                    if !first.marked_predecessor {
                        errors.push(format!(
                        "Object {}: First entry in range {}-{} should have marked_predecessor=true",
                        object_id, prune_from, prune_to
                    ));
                    }

                    if last.marked_predecessor {
                        errors.push(format!(
                        "Object {}: Last entry in range {}-{} should have marked_predecessor=false",
                        object_id, prune_from, prune_to
                    ));
                    }
                }
            }
        }

        // 3. Global consistency checks

        // Check that no checkpoint appears twice for the same object
        for (object_id, entries) in &object_entries {
            let object_id = ObjectID::from_bytes(object_id).unwrap();
            let mut seen_checkpoints = BTreeSet::new();
            for entry in entries {
                if !seen_checkpoints.insert(entry.cp_sequence_number) {
                    errors.push(format!(
                        "Object {}: Duplicate checkpoint {}",
                        object_id, entry.cp_sequence_number
                    ));
                }
            }
        }

        // 4. Efficiency checks

        // After pruning, most objects should have exactly 1-2 entries per pruned range
        let avg_entries_per_object = if stats.total_objects > 0 {
            stats.total_entries as f64 / stats.total_objects as f64
        } else {
            0.0
        };

        if avg_entries_per_object > 3.0 {
            errors.push(format!(
                "Pruning may be ineffective: average {:.1} entries per object",
                avg_entries_per_object
            ));
        }

        // No orphaned entries should exist
        if stats.orphaned_entries > 0 {
            errors.push(format!(
                "Found {} orphaned entries (marked_predecessor=true AND obsolete_at=Some)",
                stats.orphaned_entries
            ));
        }

        Ok(ValidationResult {
            valid: errors.is_empty(),
            errors,
            stats,
        })
    }
}
