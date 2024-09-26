// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_task::BackfillTask;
use crate::backfill::BackfillTaskKind;
use std::sync::Arc;

mod sql_backfill;
pub mod system_state_summary_json;

pub fn get_backfill_task(kind: BackfillTaskKind) -> Arc<dyn BackfillTask> {
    match kind {
        BackfillTaskKind::SystemStateSummaryJson => {
            Arc::new(system_state_summary_json::SystemStateSummaryJsonBackfill {})
        }
        BackfillTaskKind::Sql { sql, key_column } => {
            Arc::new(sql_backfill::SqlBackFill::new(sql, key_column))
        }
    }
}
