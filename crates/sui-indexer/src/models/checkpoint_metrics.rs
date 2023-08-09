// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use diesel::sql_types::Float8;

use crate::schema::checkpoint_metrics;

#[derive(Queryable, Insertable, Debug, Clone)]
#[diesel(table_name = checkpoint_metrics)]
pub struct CheckpointMetrics {
    pub checkpoint: i64,
    pub epoch: i64,
    pub real_time_tps: f64,
    pub peak_tps_30d: f64,
    pub rolling_total_transactions: i64,
    pub rolling_total_transaction_blocks: i64,
    pub rolling_total_successful_transactions: i64,
    pub rolling_total_successful_transaction_blocks: i64,
}

impl Default for CheckpointMetrics {
    fn default() -> Self {
        Self {
            // -1 to differentiate from first checkpoint of sequence 0
            checkpoint: -1,
            epoch: 0,
            real_time_tps: 0.0,
            peak_tps_30d: 0.0,
            rolling_total_transactions: 0,
            rolling_total_transaction_blocks: 0,
            rolling_total_successful_transactions: 0,
            rolling_total_successful_transaction_blocks: 0,
        }
    }
}

#[derive(Debug, QueryableByName)]
pub struct Tps {
    #[diesel(sql_type = Float8)]
    pub tps: f64,
}
