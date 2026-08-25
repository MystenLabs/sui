// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The typed rows the backtest emits. Both sinks (`postgres` and `ndjson`) write these: postgres
//! inserts them via diesel, the ndjson sink serializes them with serde. Counters are `i64` because
//! postgres has no unsigned integer type; the source counts are `u64` and converted at the edge.

use diesel::prelude::*;
use serde::Serialize;

use crate::schema::divergence;
use crate::schema::run_stats;

/// A transaction whose recomputed success/failure status disagrees with its on-chain status.
#[derive(Insertable, Queryable, Selectable, Serialize, Debug, Clone)]
#[diesel(table_name = divergence)]
pub struct DivergenceRow {
    pub task: String,
    pub epoch: i64,
    pub checkpoint: i64,
    pub tx_digest: String,
    pub original_status: String,
    pub original_failure_kind: Option<String>,
    pub recomputed_status: String,
    pub recomputed_error_kind: Option<String>,
    pub recomputed_error_detail: Option<String>,
    pub missing_modified: i64,
    pub missing_loaded: i64,
    pub missing_consensus: i64,
    pub digest_mismatches: i64,
}

/// Per-checkpoint replay denominators for a run.
#[derive(Insertable, Queryable, Selectable, Serialize, Debug, Clone)]
#[diesel(table_name = run_stats)]
pub struct RunStatsRow {
    pub task: String,
    pub epoch: i64,
    pub checkpoint: i64,
    pub checked: i64,
    pub executed: i64,
    pub divergences: i64,
    pub reconstruction_errors: i64,
    pub coin_reservation_skipped: i64,
    pub execute_skipped: i64,
    pub gas_from_balance: i64,
    pub cancellation_excluded: i64,
}

#[cfg(test)]
mod tests {
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_framework::Indexer;

    use super::*;
    use crate::MIGRATIONS;

    /// The backtest migrations apply on top of the framework's watermark tables, and both row types
    /// round-trip through diesel against the real schema.
    #[tokio::test]
    async fn migrations_apply_and_rows_round_trip() {
        let (indexer, _temp_db) = Indexer::new_for_testing(&MIGRATIONS).await;
        let mut conn = indexer.store().connect().await.unwrap();

        let div = DivergenceRow {
            task: "t".to_owned(),
            epoch: 1152,
            checkpoint: 284396922,
            tx_digest: "digest".to_owned(),
            original_status: "success".to_owned(),
            original_failure_kind: None,
            recomputed_status: "failure".to_owned(),
            recomputed_error_kind: Some("InvalidLinkage".to_owned()),
            recomputed_error_detail: Some("conflicting resolutions".to_owned()),
            missing_modified: 0,
            missing_loaded: 0,
            missing_consensus: 0,
            digest_mismatches: 0,
        };
        diesel::insert_into(divergence::table)
            .values(&div)
            .execute(&mut conn)
            .await
            .unwrap();

        let stats = RunStatsRow {
            task: "t".to_owned(),
            epoch: 1152,
            checkpoint: 284396922,
            checked: 10,
            executed: 10,
            divergences: 1,
            reconstruction_errors: 0,
            coin_reservation_skipped: 0,
            execute_skipped: 0,
            gas_from_balance: 2,
            cancellation_excluded: 0,
        };
        diesel::insert_into(run_stats::table)
            .values(&stats)
            .execute(&mut conn)
            .await
            .unwrap();

        let divs: Vec<DivergenceRow> = divergence::table.load(&mut conn).await.unwrap();
        assert_eq!(divs.len(), 1);
        assert_eq!(
            divs[0].recomputed_error_kind.as_deref(),
            Some("InvalidLinkage")
        );

        let all_stats: Vec<RunStatsRow> = run_stats::table.load(&mut conn).await.unwrap();
        assert_eq!(all_stats.len(), 1);
        assert_eq!(all_stats[0].divergences, 1);
    }
}
