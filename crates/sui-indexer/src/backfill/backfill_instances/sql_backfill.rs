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

        // If there is already a `WHERE` clause in the SQL, apply the range as an `AND` clause.
        let range_condition = format!(
            "{} BETWEEN {} AND {}",
            self.key_column,
            *range.start(),
            *range.end()
        );

        let mut query = if self.sql.to_uppercase().contains("WHERE") {
            format!("{} AND {}", self.sql, range_condition)
        } else {
            format!("{} WHERE {}", self.sql, range_condition)
        };

        query = if self.sql.to_uppercase().contains("INSERT") {
            format!("{} ON CONFLICT DO NOTHING", query)
        } else {
            query
        };

        diesel::sql_query(query).execute(&mut conn).await.unwrap();
    }
}
