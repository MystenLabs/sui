// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_task::BackfillTask;
use crate::database::ConnectionPool;
use crate::schema::full_objects_history::dsl::full_objects_history;
use crate::schema::objects_history::dsl::*;
use async_trait::async_trait;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl;
use std::ops::RangeInclusive;

pub struct FullObjectsHistoryBackfill {}

#[async_trait]
impl BackfillTask for FullObjectsHistoryBackfill {
    async fn backfill_range(pool: ConnectionPool, range: &RangeInclusive<usize>) {
        let mut conn = pool.get().await.unwrap();

        let selected_rows = objects_history
            .filter(checkpoint_sequence_number.between(*range.start() as i64, *range.end() as i64))
            .select((object_id, object_version, serialized_object));

        diesel::insert_into(full_objects_history)
            .values(selected_rows)
            .on_conflict_do_nothing()
            .execute(&mut conn)
            .await
            .unwrap();
    }
}
