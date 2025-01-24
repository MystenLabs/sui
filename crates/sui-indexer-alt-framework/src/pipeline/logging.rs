// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

use tracing::{debug, info};

use crate::models::watermarks::{CommitterWatermark, PrunerWatermark};

use super::Processor;

/// Tracing message for the watermark update will be logged at info level at least this many
/// checkpoints.
const LOUD_WATERMARK_UPDATE_INTERVAL: i64 = 5 * 10;

#[derive(Default)]
pub(crate) struct LoggerWatermark {
    checkpoint: i64,
    transaction: Option<i64>,
}

pub(crate) struct WatermarkLogger {
    name: &'static str,
    timer: Instant,
    prev_watermark: LoggerWatermark,
}

impl WatermarkLogger {
    pub fn new(name: &'static str, init_watermark: impl Into<LoggerWatermark>) -> Self {
        Self {
            name,
            timer: Instant::now(),
            prev_watermark: init_watermark.into(),
        }
    }

    /// Log the watermark update.
    /// `watermark_update_latency` is the time spent to update the watermark.
    ///
    /// Given the new watermark, the logger will compare with the previous watermark to compute the
    /// average TPS (transactions per second) and CPS (checkpoints per second) since the last update.
    ///
    /// If the watermark update is less than `LOUD_WATERMARK_UPDATE_INTERVAL` checkpoints apart,
    /// the log message will be at debug level. Otherwise, it will be at info level.
    pub fn log<H: Processor>(
        &mut self,
        watermark: impl Into<LoggerWatermark>,
        watermark_update_latency: f64,
    ) {
        let watermark: LoggerWatermark = watermark.into();
        let logger_timer_elapsed = self.timer.elapsed().as_secs_f64();
        let realtime_average_tps = match (self.prev_watermark.transaction, watermark.transaction) {
            (Some(prev), Some(curr)) => Some((curr - prev) as f64 / logger_timer_elapsed),
            _ => None,
        };
        let realtime_average_cps =
            (watermark.checkpoint - self.prev_watermark.checkpoint) as f64 / logger_timer_elapsed;

        if watermark.checkpoint < self.prev_watermark.checkpoint + LOUD_WATERMARK_UPDATE_INTERVAL {
            debug!(
                logger = self.name,
                pipeline = H::NAME,
                checkpoint = watermark.checkpoint,
                transaction = watermark.transaction,
                tps = realtime_average_tps,
                cps = realtime_average_cps,
                elapsed_ms = format!("{:.3}", watermark_update_latency * 1000.0),
                "Updated watermark",
            );
            return;
        }

        info!(
            logger = self.name,
            pipeline = H::NAME,
            checkpoint = watermark.checkpoint,
            transaction = watermark.transaction,
            tps = realtime_average_tps,
            cps = realtime_average_cps,
            elapsed_ms = format!("{:.3}", watermark_update_latency * 1000.0),
            "Updated watermark",
        );
        self.prev_watermark = watermark;
        self.timer = Instant::now();
    }
}

impl From<&CommitterWatermark<'_>> for LoggerWatermark {
    fn from(watermark: &CommitterWatermark) -> Self {
        Self {
            checkpoint: watermark.checkpoint_hi_inclusive,
            transaction: Some(watermark.tx_hi),
        }
    }
}

impl From<&PrunerWatermark<'_>> for LoggerWatermark {
    fn from(watermark: &PrunerWatermark) -> Self {
        Self {
            checkpoint: watermark.pruner_hi,
            transaction: None,
        }
    }
}
