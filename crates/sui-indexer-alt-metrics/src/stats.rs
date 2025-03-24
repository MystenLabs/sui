// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_pg_db::Db;

pub struct DbConnectionStatsSnapshot {
    // From state
    pub connections: usize,
    pub idle_connections: usize,

    // From stats
    pub get_direct: u64,
    pub get_waited: u64,
    pub get_timed_out: u64,
    pub get_wait_time_ms: u64, // Converted from Duration to milliseconds
    pub connections_created: u64,
    pub connections_closed_broken: u64,
    pub connections_closed_invalid: u64,
    pub connections_closed_max_lifetime: u64,
    pub connections_closed_idle_timeout: u64,
}

pub trait DbConnectionStats: Send + Sync {
    fn get_connection_stats(&self) -> DbConnectionStatsSnapshot;
}

impl DbConnectionStats for Db {
    fn get_connection_stats(&self) -> DbConnectionStatsSnapshot {
        let state = self.state();
        let stats = state.statistics;
        DbConnectionStatsSnapshot {
            connections: state.connections as usize,
            idle_connections: state.idle_connections as usize,
            get_direct: stats.get_direct as u64,
            get_waited: stats.get_waited as u64,
            get_timed_out: stats.get_timed_out as u64,
            get_wait_time_ms: stats.get_wait_time.as_millis() as u64,
            connections_created: stats.connections_created as u64,
            connections_closed_broken: stats.connections_closed_broken,
            connections_closed_invalid: stats.connections_closed_invalid,
            connections_closed_max_lifetime: stats.connections_closed_max_lifetime,
            connections_closed_idle_timeout: stats.connections_closed_idle_timeout,
        }
    }
}
