// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use diesel::sql_types::BigInt;

use crate::schema_v2::network_metrics;

#[derive(Clone, Debug, Default, Queryable, Insertable)]
#[diesel(table_name = network_metrics)]
pub struct StoredNetworkMetrics {
    pub checkpoint: i64,
    pub epoch: i64,
    pub timestamp_ms: i64,
    pub real_time_tps: f64,
    pub peak_tps_30d: f64,
    pub total_addresses: i64,
    pub total_objects: i64,
    pub total_packages: i64,
}

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct RowCountEstimation {
    #[diesel(sql_type = BigInt)]
    pub estimated_count: i64,
}
