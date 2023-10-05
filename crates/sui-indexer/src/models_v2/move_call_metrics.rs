// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Text};
use diesel::QueryableByName;

use crate::schema_v2::{move_call_metrics, move_calls};

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = move_calls)]
pub struct StoredMoveCall {
    pub id: Option<i64>,
    pub transaction_sequence_number: i64,
    pub checkpoint_sequence_number: i64,
    pub epoch: i64,
    pub move_package: String,
    pub move_module: String,
    pub move_function: String,
}

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = move_call_metrics)]
pub struct StoredMoveCallMetrics {
    pub id: Option<i64>,
    pub checkpoint: i64,
    pub epoch: i64,
    pub day: i64,
    pub move_package: String,
    pub move_module: String,
    pub move_function: String,
    pub count: i64,
}

pub struct DerivedMoveCallInfo {
    pub tx_sequence_number: i64,
    pub checkpoint: i64,
    pub move_package: String,
    pub move_module: String,
    pub move_function: String,
}

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct QueriedMoveMetrics {
    #[diesel(sql_type = BigInt)]
    pub epoch: i64,
    #[diesel(sql_type = BigInt)]
    pub day: i64,
    #[diesel(sql_type = Text)]
    pub move_package: String,
    #[diesel(sql_type = Text)]
    pub move_module: String,
    #[diesel(sql_type = Text)]
    pub move_function: String,
    #[diesel(sql_type = BigInt)]
    pub count: i64,
}

pub fn build_move_call_metric_query(epoch: i64, days: i64) -> String {
    format!("SELECT {} AS epoch, {}::BIGINT AS day, move_package, move_module, move_function, COUNT(*) AS count
        FROM move_calls
        WHERE epoch >= {}
        GROUP BY move_package, move_module, move_function
        ORDER BY count DESC
        LIMIT 10;", epoch, days, epoch - days)
}
