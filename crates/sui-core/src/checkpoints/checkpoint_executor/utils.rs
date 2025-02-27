// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Instant};

use crate::checkpoints::CheckpointStore;
use crate::execution_cache::TransactionCacheRead;
use futures::{future::Either, Stream};
use mysten_common::fatal;
use std::time::Duration;
use sui_types::{
    base_types::{TransactionDigest, TransactionEffectsDigest},
    messages_checkpoint::{CheckpointSequenceNumber, VerifiedCheckpoint},
};
use tracing::{debug, error, info, instrument, warn};

#[instrument(level = "debug", skip_all)]
pub(super) fn stream_synced_checkpoints(
    checkpoint_store: Arc<CheckpointStore>,
    start_seq: CheckpointSequenceNumber,
    stop_seq: Option<CheckpointSequenceNumber>,
) -> impl Stream<Item = VerifiedCheckpoint> + 'static {
    let scheduling_timeout_config = get_scheduling_timeout();
    let panic_timeout = scheduling_timeout_config.panic_timeout;
    let warning_timeout = scheduling_timeout_config.warning_timeout;

    struct State {
        current_seq: CheckpointSequenceNumber,
        checkpoint_store: Arc<CheckpointStore>,
        warning_timeout: Duration,
        panic_timeout: Option<Duration>,
        stop_seq: Option<CheckpointSequenceNumber>,
    }

    let state = State {
        current_seq: start_seq,
        checkpoint_store,
        warning_timeout,
        panic_timeout,
        stop_seq,
    };

    futures::stream::unfold(Some(state), |state| async move {
        match state {
            None => None,
            Some(state) if state.current_seq > state.stop_seq.unwrap_or(u64::MAX) => None,
            Some(mut state) => {
                let seq = state.current_seq;
                let checkpoint = wait_for_checkpoint(
                    &state.checkpoint_store,
                    seq,
                    state.warning_timeout,
                    state.panic_timeout,
                )
                .await;
                info!(
                    "received synced checkpoint: {:?}",
                    checkpoint.sequence_number
                );
                if checkpoint.end_of_epoch_data.is_some() {
                    Some((checkpoint, None))
                } else {
                    state.current_seq = seq + 1;
                    Some((checkpoint, Some(state)))
                }
            }
        }
    })
}

async fn wait_for_checkpoint(
    checkpoint_store: &CheckpointStore,
    seq: CheckpointSequenceNumber,
    warning_timeout: Duration,
    panic_timeout: Option<Duration>,
) -> VerifiedCheckpoint {
    debug!("waiting for checkpoint: {:?}", seq);
    loop {
        tokio::select! {
            checkpoint = checkpoint_store.notify_read_synced_checkpoint(seq) => {
                return checkpoint;
            }

            _ = tokio::time::sleep(warning_timeout) => {
                warn!(
                    "Received no new synced checkpoints for {warning_timeout:?}. Next checkpoint to be scheduled: {seq}",
                );
            }

            _ = panic_timeout
                        .map(|d| Either::Left(tokio::time::sleep(d)))
                        .unwrap_or_else(|| Either::Right(futures::future::pending())) => {
                fatal!("No new synced checkpoints received for {panic_timeout:?}");
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CheckpointTimeoutConfig {
    pub panic_timeout: Option<Duration>,
    pub warning_timeout: Duration,
}

// We use a thread local so that the config can be overridden on a per-test basis. This means
// that get_scheduling_timeout() can be called multiple times in a multithreaded context, but
// the function is still very cheap to call so this is okay.
thread_local! {
    static SCHEDULING_TIMEOUT: once_cell::sync::OnceCell<CheckpointTimeoutConfig> =
        const { once_cell::sync::OnceCell::new() };
}

#[cfg(msim)]
pub fn init_checkpoint_timeout_config(config: CheckpointTimeoutConfig) {
    SCHEDULING_TIMEOUT.with(|s| {
        s.set(config).expect("SchedulingTimeoutConfig already set");
    });
}

fn get_scheduling_timeout() -> CheckpointTimeoutConfig {
    fn inner() -> CheckpointTimeoutConfig {
        let panic_timeout: Option<Duration> = if cfg!(msim) {
            Some(Duration::from_secs(45))
        } else {
            std::env::var("NEW_CHECKPOINT_PANIC_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_millis)
        };

        let warning_timeout: Duration = std::env::var("NEW_CHECKPOINT_WARNING_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(5));

        CheckpointTimeoutConfig {
            panic_timeout,
            warning_timeout,
        }
    }

    SCHEDULING_TIMEOUT.with(|s| *s.get_or_init(inner))
}

pub(super) fn assert_not_forked(
    checkpoint: &VerifiedCheckpoint,
    tx_digest: &TransactionDigest,
    expected_digest: &TransactionEffectsDigest,
    actual_effects_digest: &TransactionEffectsDigest,
    cache_reader: &dyn TransactionCacheRead,
) {
    if *expected_digest != *actual_effects_digest {
        let actual_effects = cache_reader
            .get_executed_effects(tx_digest)
            .expect("actual effects should exist");

        // log observed effects (too big for panic message) and then panic.
        error!(
            ?checkpoint,
            ?tx_digest,
            ?expected_digest,
            ?actual_effects,
            "fork detected!"
        );
        panic!(
            "When executing checkpoint {}, transaction {} \
            is expected to have effects digest {}, but got {}!",
            checkpoint.sequence_number(),
            tx_digest,
            expected_digest,
            actual_effects_digest,
        );
    }
}

#[derive(Default)]
pub(super) struct TPSEstimator {
    last_update: Option<Instant>,
    transaction_count: u64,
    tps: f64,
}

impl TPSEstimator {
    pub fn update(&mut self, now: Instant, transaction_count: u64) -> f64 {
        if let Some(last_update) = self.last_update {
            if now > last_update {
                let delta_t = now.duration_since(last_update);
                let delta_c = transaction_count - self.transaction_count;
                let tps = delta_c as f64 / delta_t.as_secs_f64();
                self.tps = self.tps * 0.9 + tps * 0.1;
            }
        }

        self.last_update = Some(now);
        self.transaction_count = transaction_count;
        self.tps
    }
}
