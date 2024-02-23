// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Double, Float8};

use sui_json_rpc_types::NetworkMetrics;

use crate::schema::epoch_peak_tps;

#[derive(Clone, Debug, Queryable, Insertable)]
#[diesel(table_name = epoch_peak_tps)]
pub struct StoredEpochPeakTps {
    pub epoch: i64,
    pub peak_tps: f64,
    pub peak_tps_30d: f64,
}

impl Default for StoredEpochPeakTps {
    fn default() -> Self {
        Self {
            epoch: -1,
            peak_tps: 0.0,
            peak_tps_30d: 0.0,
        }
    }
}

#[derive(QueryableByName, Debug, Clone, Default)]
pub struct StoredNetworkMetrics {
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

impl From<StoredNetworkMetrics> for NetworkMetrics {
    fn from(db: StoredNetworkMetrics) -> Self {
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

#[derive(Debug, QueryableByName)]
pub struct Tps {
    #[diesel(sql_type = Float8)]
    pub peak_tps: f64,
}
