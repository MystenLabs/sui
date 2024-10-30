// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_task::BackfillTask;
use crate::database::ConnectionPool;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use std::ops::RangeInclusive;

pub struct SqlBackFill {
    sql: String,
    key_column: String,
}

impl SqlBackFill {
    pub fn new(sql: String, key_column: String) -> Self {
        Self { sql, key_column }
    }
}

#[async_trait]
impl BackfillTask for SqlBackFill {
    async fn backfill_range(&self, pool: ConnectionPool, range: &RangeInclusive<usize>) {
        let mut conn = pool.get().await.unwrap();

        let query = format!(
            "{} WHERE {} BETWEEN {} AND {} ON CONFLICT DO NOTHING",
            self.sql,
            self.key_column,
            *range.start(),
            *range.end()
        );

        diesel::sql_query(query).execute(&mut conn).await.unwrap();
    }
}
