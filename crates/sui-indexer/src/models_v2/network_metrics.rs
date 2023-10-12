// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use diesel::sql_types::BigInt;

use sui_json_rpc_types::NetworkMetrics;

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

impl Into<NetworkMetrics> for StoredNetworkMetrics {
    fn into(self) -> NetworkMetrics {
        NetworkMetrics {
            current_checkpoint: self.checkpoint as u64,
            current_epoch: self.epoch as u64,
            current_tps: self.real_time_tps,
            tps_30_days: self.peak_tps_30d,
            total_addresses: self.total_addresses as u64,
            total_objects: self.total_objects as u64,
            total_packages: self.total_packages as u64,
        }
    }
}
