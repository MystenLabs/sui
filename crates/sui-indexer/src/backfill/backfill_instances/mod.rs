// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_instances::ingestion_backfills::ingestion_backfill_task::IngestionBackfillTask;
use crate::backfill::backfill_instances::ingestion_backfills::raw_checkpoints::RawCheckpointsBackFill;
use crate::backfill::backfill_task::BackfillTask;
use crate::backfill::{BackfillTaskKind, IngestionBackfillKind};
use std::sync::Arc;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

mod ingestion_backfills;
mod sql_backfill;
mod system_state_summary_json;
mod tx_affected_objects;

pub async fn get_backfill_task(
    kind: BackfillTaskKind,
    range_start: usize,
) -> Arc<dyn BackfillTask> {
    match kind {
        BackfillTaskKind::SystemStateSummaryJson => {
            Arc::new(system_state_summary_json::SystemStateSummaryJsonBackfill)
        }
        BackfillTaskKind::TxAffectedObjects => {
            Arc::new(tx_affected_objects::TxAffectedObjectsBackfill)
        }
        BackfillTaskKind::Sql { sql, key_column } => {
            Arc::new(sql_backfill::SqlBackFill::new(sql, key_column))
        }
        BackfillTaskKind::Ingestion {
            kind,
            remote_store_url,
        } => match kind {
            IngestionBackfillKind::RawCheckpoints => Arc::new(
                IngestionBackfillTask::<RawCheckpointsBackFill>::new(
                    remote_store_url,
                    range_start as CheckpointSequenceNumber,
                )
                .await,
            ),
        },
    }
}
