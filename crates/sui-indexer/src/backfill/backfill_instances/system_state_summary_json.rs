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

pub struct SystemStateSummaryJsonBackfill {}

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

        let system_states = results.into_iter().filter_map(|bytes| {
            let bytes = bytes?;
            let system_state_summary: SuiSystemStateSummary = bcs::from_bytes(&bytes).unwrap();
            let json_ser = serde_json::to_value(&system_state_summary).unwrap();
            Some((system_state_summary.epoch, json_ser))
        });
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
