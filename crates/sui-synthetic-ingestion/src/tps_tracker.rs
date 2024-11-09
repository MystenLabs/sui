// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::IndexerProgress;
use std::time::{Duration, Instant};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tracing::info;

pub(crate) struct TpsTracker {
    start_time: Instant,
    starting_state: Option<IndexerProgress>,

    prev_time: Instant,
    prev_timed_state: Option<IndexerProgress>,

    cur_state: Option<IndexerProgress>,

    peak_tps: f64,

    /// Log time elapsed and TPS every log_frequency duration.
    log_frequency: Duration,
}

impl TpsTracker {
    pub fn new(log_frequency: Duration) -> Self {
        let start_time = Instant::now();
        Self {
            start_time,
            starting_state: None,
            prev_time: start_time,
            prev_timed_state: None,
            cur_state: None,
            peak_tps: 0.0,
            log_frequency,
        }
    }

    pub fn update(&mut self, cur_state: IndexerProgress) {
        self.cur_state = Some(cur_state.clone());
        let cur_time = Instant::now();
        let Some(prev_timed_state) = self.prev_timed_state.clone() else {
            self.prev_time = cur_time;
            self.prev_timed_state = Some(cur_state.clone());
            self.start_time = cur_time;
            self.starting_state = Some(cur_state);
            return;
        };
        let elapsed = cur_time - self.prev_time;
        if elapsed < self.log_frequency {
            return;
        }
        let tps = (cur_state.network_total_transactions
            - prev_timed_state.network_total_transactions) as f64
            / elapsed.as_secs_f64();
        let cps =
            (cur_state.checkpoint - prev_timed_state.checkpoint) as f64 / elapsed.as_secs_f64();
        info!(
            "Last processed checkpoint: {}, Current TPS: {:.2}, CPS: {:.2}",
            cur_state.checkpoint, tps, cps
        );
        self.peak_tps = self.peak_tps.max(tps);
        self.prev_time = cur_time;
        self.prev_timed_state = Some(cur_state);
    }

    pub fn finish(&mut self) -> CheckpointSequenceNumber {
        let elapsed = Instant::now() - self.start_time;
        let cur_state = self.cur_state.clone().unwrap();
        let starting_state = self.starting_state.clone().unwrap();
        let tps = (cur_state.network_total_transactions - starting_state.network_total_transactions)
            as f64
            / elapsed.as_secs_f64();
        let cps = (cur_state.checkpoint - starting_state.checkpoint) as f64 / elapsed.as_secs_f64();
        info!(
            "Benchmark completed. Total time: {:?}, Average TPS: {:.2}, CPS: {:.2}. Peak TPS: {:.2}",
            elapsed, tps, cps, self.peak_tps,
        );
        cur_state.checkpoint
    }
}
