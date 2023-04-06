// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::sql_types::BigInt;
use diesel::sql_types::Double;
use diesel::sql_types::Text;
use diesel::QueryableByName;

use sui_json_rpc_types::NetworkMetrics;

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct DBNetworkMetrics {
    #[diesel(sql_type = Double)]
    pub current_tps: f64,
    #[diesel(sql_type = Double)]
    pub tps_30_days: f64,
    #[diesel(sql_type = BigInt)]
    pub total_packages: i64,
    #[diesel(sql_type = BigInt)]
    pub total_addresses: i64,
    #[diesel(sql_type = BigInt)]
    pub total_objects: i64,
    #[diesel(sql_type = BigInt)]
    pub current_epoch: i64,
    #[diesel(sql_type = BigInt)]
    pub current_checkpoint: i64,
}

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct DBMoveCallMetrics {
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

impl From<DBNetworkMetrics> for NetworkMetrics {
    fn from(db: DBNetworkMetrics) -> Self {
        Self {
            current_tps: db.current_tps,
            tps_30_days: db.tps_30_days,
            total_packages: db.total_packages as u64,
            total_addresses: db.total_addresses as u64,
            total_objects: db.total_objects as u64,
            current_epoch: db.current_epoch as u64,
            current_checkpoint: db.current_checkpoint as u64,
        }
    }
}
