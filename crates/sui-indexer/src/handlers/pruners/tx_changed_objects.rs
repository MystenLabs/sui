// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use diesel::ExpressionMethods;
use diesel::{dsl::max, QueryDsl};
use diesel_async::RunQueryDsl;

use crate::{
    database::Connection, execute_delete_range_query, handlers::pruner::PrunableTable,
    schema::tx_changed_objects,
};

use super::Prunable;

pub struct TxChangedObjects;

#[async_trait::async_trait]
impl Prunable for TxChangedObjects {
    const NAME: PrunableTable = PrunableTable::TxChangedObjects;

    const CHUNK_SIZE: u64 = 100_000;

    async fn data_lo(conn: &mut Connection<'_>) -> anyhow::Result<u64> {
        Ok(tx_changed_objects::table
            .select(max(tx_changed_objects::tx_sequence_number))
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
            tx_changed_objects,
            tx_sequence_number,
            prune_lo,
            prune_hi
        )
        .context(format!("Failed to prune {}", Self::NAME.as_ref()))
    }
}
