// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::sql_query;
use diesel_async::RunQueryDsl;
use sui_pg_db::Connection;

use super::PruningStrategyTrait;

pub struct SimpleRangePruning {
    pub table_name: String,
    pub key_column_name: String,
}

#[async_trait::async_trait]
impl PruningStrategyTrait for SimpleRangePruning {
    fn requires_processed_values(&self) -> bool {
        false
    }

    async fn prune(
        &self,
        from: u64,
        to_exclusive: u64,
        conn: &mut Connection,
    ) -> anyhow::Result<usize> {
        let query = format!(
            "DELETE FROM {} WHERE {} >= {} AND {} < {}",
            self.table_name, self.key_column_name, from, self.key_column_name, to_exclusive
        );
        let rows_deleted = sql_query(query).execute(conn).await?;
        Ok(rows_deleted)
    }
}
