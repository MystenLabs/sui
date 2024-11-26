// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use tracing::{debug, info};

use super::Processor;

/// Tracing message for the watermark update will be logged at info level at least this many
/// checkpoints.
const LOUD_WATERMARK_UPDATE_INTERVAL: i64 = 5 * 10;

pub(crate) struct WatermarkLogger {
    name: &'static str,
    timer: Instant,
    prev_checkpoint: i64,
    prev_transaction: Option<i64>,
}

impl WatermarkLogger {
    pub fn new(name: &'static str, init_checkpoint: i64, init_transaction: Option<i64>) -> Self {
        Self {
            name,
            timer: Instant::now(),
            prev_checkpoint: init_checkpoint,
            prev_transaction: init_transaction,
        }
    }

    pub fn log<H: Processor>(
        &mut self,
        checkpoint: i64,
        transaction: Option<i64>,
        watermark_update_latency: f64,
    ) {
        let elapsed = self.timer.elapsed().as_secs_f64();
        let realtime_average_tps = match (self.prev_transaction, transaction) {
            (Some(prev), Some(curr)) => Some((curr - prev) as f64 / elapsed),
            _ => None,
        };
        let realtime_average_cps = (checkpoint - self.prev_checkpoint) as f64 / elapsed;

        if checkpoint < self.prev_checkpoint + LOUD_WATERMARK_UPDATE_INTERVAL {
            debug!(
                logger_name = self.name,
                pipeline = H::NAME,
                checkpoint,
                transaction,
                realtime_average_tps,
                realtime_average_cps,
                watermark_update_latency,
                "Watermark",
            );
            return;
        }

        info!(
            logger_name = self.name,
            pipeline = H::NAME,
            checkpoint,
            transaction,
            realtime_average_tps,
            realtime_average_cps,
            watermark_update_latency,
            "Watermark",
        );
        self.prev_checkpoint = checkpoint;
        self.prev_transaction = transaction;
        self.timer = Instant::now();
    }
}
