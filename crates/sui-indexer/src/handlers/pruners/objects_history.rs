// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use diesel::{prelude::QueryableByName, sql_types::BigInt};
use diesel_async::RunQueryDsl;

use crate::database::Connection;

use super::Pruner;

pub struct ObjectsHistory;

pub fn get_partition_sql(table_name: &str) -> String {
    format!(
        r"
        SELECT
            MIN(SUBSTRING(child.relname FROM '\d+$'))::integer as first_partition
        FROM pg_inherits
        JOIN pg_class parent ON pg_inherits.inhparent = parent.oid
        JOIN pg_class child ON pg_inherits.inhrelid = child.oid
        WHERE parent.relname = '{}';
        ",
        table_name
    )
}

#[derive(QueryableByName, Debug, Clone)]
struct PartitionedTable {
    #[diesel(sql_type = BigInt)]
    first_partition: i64,
}

#[async_trait::async_trait]
impl Pruner for ObjectsHistory {
    const NAME: &'static str = "objects_history";

    const BATCH_SIZE: usize = 100;
    const CHUNK_SIZE: usize = 1000;
    const MAX_PENDING_SIZE: usize = 10000;

    async fn data_lo(conn: &mut Connection<'_>) -> anyhow::Result<u64> {
        diesel::sql_query(get_partition_sql(Self::NAME))
            .get_result::<PartitionedTable>(conn)
            .await
            .map(|entry| entry.first_partition as u64)
            .context("failed to get first partition")
    }

    async fn prune(
        prune_lo: u64,
        _prune_hi: u64,
        conn: &mut Connection<'_>,
    ) -> anyhow::Result<usize> {
        diesel::sql_query("CALL drop_partition($1, $2)")
            .bind::<diesel::sql_types::Text, _>(Self::NAME.to_string())
            .bind::<diesel::sql_types::BigInt, _>(prune_lo as i64)
            .execute(conn)
            .await
            .context("failed to drop partition")
    }
}

// TODO: I think for pruner we just need to know FANOUT, and how many to delete at once
// I think generally, unpartitioned tables can have CHUNK_SIZE=100k, and partitioned tables would be 1 at a time
