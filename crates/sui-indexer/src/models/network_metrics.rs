// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::sql_types::BigInt;
use diesel::sql_types::Double;
use diesel::QueryableByName;

use sui_json_rpc_types::NetworkMetrics;

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct DBNetworkMetrics {
    #[diesel(sql_type = Double)]
    pub current_tps: f64,
    #[diesel(sql_type = Double)]
    pub tps_30_days: f64,
    #[diesel(sql_type = Double)]
    pub current_cps: f64,
    #[diesel(sql_type = Double)]
    pub cps_30_days: f64,
    #[diesel(sql_type = BigInt)]
    pub total_packages: i64,
    #[diesel(sql_type = BigInt)]
    pub total_addresses: i64,
    #[diesel(sql_type = BigInt)]
    pub total_objects: i64,
}

impl From<DBNetworkMetrics> for NetworkMetrics {
    fn from(db: DBNetworkMetrics) -> Self {
        Self {
            current_tps: db.current_tps,
            tps_30_days: db.tps_30_days,
            current_cps: db.current_cps,
            cps_30_days: db.cps_30_days,
            total_packages: db.total_packages as u64,
            total_addresses: db.total_addresses as u64,
            total_objects: db.total_objects as u64,
        }
    }
}
