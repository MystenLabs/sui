// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use diesel::ExpressionMethods;
use diesel::{dsl::max, QueryDsl};
use diesel_async::RunQueryDsl;

use crate::{
    database::Connection, execute_delete_range_query, handlers::pruner::PrunableTable,
    schema::tx_calls_pkg,
};

use super::Prunable;

pub struct TxCallsPkg;

#[async_trait::async_trait]
impl Prunable for TxCallsPkg {
    const NAME: PrunableTable = PrunableTable::TxCallsPkg;

    const CHUNK_SIZE: u64 = 100_000;

    async fn data_lo(conn: &mut Connection<'_>) -> anyhow::Result<u64> {
        Ok(tx_calls_pkg::table
            .select(max(tx_calls_pkg::tx_sequence_number))
            .first::<Option<i64>>(conn)
            .await
            .context("Failed to find latest tx_sequence_number")?
            .unwrap_or_default() as u64)
    }

    async fn prune(
        prune_lo: u64,
        prune_hi: u64,
        mut conn: &mut Connection<'_>,
    ) -> anyhow::Result<usize> {
        execute_delete_range_query!(
            &mut conn,
            tx_calls_pkg,
            tx_sequence_number,
            prune_lo,
            prune_hi
        )
        .context(format!("Failed to prune {}", Self::NAME.as_ref()))
    }
}
