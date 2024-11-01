// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_task::BackfillTask;
use crate::database::ConnectionPool;
use crate::schema::epochs;
use async_trait::async_trait;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::{AsyncConnection, RunQueryDsl};
use std::ops::RangeInclusive;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;

pub struct SystemStateSummaryJsonBackfill;

#[async_trait]
impl BackfillTask for SystemStateSummaryJsonBackfill {
    async fn backfill_range(&self, pool: ConnectionPool, range: &RangeInclusive<usize>) {
        let mut conn = pool.get().await.unwrap();

        let results: Vec<Option<Vec<u8>>> = epochs::table
            .select(epochs::system_state)
            .filter(epochs::epoch.between(*range.start() as i64, *range.end() as i64))
            .load(&mut conn)
            .await
            .unwrap();

        let mut system_states = vec![];
        for bytes in results {
            let Some(bytes) = bytes else {
                continue;
            };
            let system_state_summary: SuiSystemStateSummary = bcs::from_bytes(&bytes).unwrap();
            let json_ser = serde_json::to_value(&system_state_summary).unwrap();
            if system_state_summary.epoch == 1 {
                // Each existing system state's epoch is off by 1.
                // This means there won't be any row with a system state summary for epoch 0.
                // We need to manually insert a row for epoch 0.
                system_states.push((0, json_ser.clone()));
            }
            system_states.push((system_state_summary.epoch, json_ser));
        }
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            Box::pin(async move {
                for (epoch, json_ser) in system_states {
                    diesel::update(epochs::table.filter(epochs::epoch.eq(epoch as i64)))
                        .set(epochs::system_state_summary_json.eq(Some(json_ser)))
                        .execute(conn)
                        .await?;
                }
                Ok(())
            })
        })
        .await
        .unwrap();
    }
}
