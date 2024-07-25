// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    register_histogram_with_registry, register_int_gauge_with_registry, Histogram, IntGauge,
    Registry,
};
use std::sync::Arc;
use sui_types::{committee::EpochId, crypto::RandomnessRound};
use tap::Pipe;

#[derive(Clone)]
pub(super) struct Metrics(Option<Arc<Inner>>);

impl std::fmt::Debug for Metrics {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Metrics").finish()
    }
}

impl Metrics {
    pub fn enabled(registry: &Registry) -> Self {
        Metrics(Some(Inner::new(registry)))
    }

    pub fn disabled() -> Self {
        Metrics(None)
    }

    pub fn set_epoch(&self, epoch: EpochId) {
        if let Some(inner) = &self.0 {
            inner.current_epoch.set(epoch as i64);
            inner.highest_round_generated.set(-1);
            inner.num_ignored_byzantine_peers.set(0);
        }
    }

    pub fn record_completed_round(&self, round: RandomnessRound) {
        if let Some(inner) = &self.0 {
            inner
                .highest_round_generated
                .set(inner.highest_round_generated.get().max(round.0 as i64));
        }
    }

    pub fn set_num_rounds_pending(&self, num_rounds_pending: i64) {
        if let Some(inner) = &self.0 {
            inner.num_rounds_pending.set(num_rounds_pending);
        }
    }

    pub fn num_rounds_pending(&self) -> Option<i64> {
        self.0.as_ref().map(|inner| inner.num_rounds_pending.get())
    }

    pub fn round_generation_latency_metric(&self) -> Option<&Histogram> {
        self.0.as_ref().map(|inner| &inner.round_generation_latency)
    }

    pub fn round_observation_latency_metric(&self) -> Option<&Histogram> {
        self.0
            .as_ref()
            .map(|inner| &inner.round_observation_latency)
    }

    pub fn inc_num_ignored_byzantine_peers(&self) {
        if let Some(inner) = &self.0 {
            inner.num_ignored_byzantine_peers.inc();
        }
    }
}

struct Inner {
    current_epoch: IntGauge,
    highest_round_generated: IntGauge,
    num_rounds_pending: IntGauge,
    round_generation_latency: Histogram,
    round_observation_latency: Histogram,
    num_ignored_byzantine_peers: IntGauge,
}

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2, 1.4,
    1.6, 1.8, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0, 9.5, 10.,
    12.5, 15., 17.5, 20., 25., 30., 60., 90., 120., 180., 300.,
];

impl Inner {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Self {
            current_epoch: register_int_gauge_with_registry!(
                "randomness_current_epoch",
                "The current epoch for which randomness is being generated (only updated after DKG completes)",
                registry
            ).unwrap(),
            highest_round_generated: register_int_gauge_with_registry!(
                "randomness_highest_round_generated",
                "The highest round for which randomness has been generated for the current epoch",
                registry
            ).unwrap(),
            num_rounds_pending: register_int_gauge_with_registry!(
                "randomness_num_rounds_pending",
                "The number of rounds of randomness that are pending generation/observation",
                registry
            ).unwrap(),
            round_generation_latency: register_histogram_with_registry!(
                "randomness_round_generation_latency",
                "Time taken to generate a single round of randomness, from when the round is requested to when the full signature is aggregated",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            round_observation_latency: register_histogram_with_registry!(
                "randomness_round_observation_latency",
                "Time taken from when partial signatures are sent for a round of randomness to when the value is observed in an executed checkpoint",
                LATENCY_SEC_BUCKETS.to_vec(),
                registry
            ).unwrap(),
            num_ignored_byzantine_peers: register_int_gauge_with_registry!(
                "randomness_num_ignored_byzantine_peers",
                "The number of byzantine peers that have been ignored by the randomness newtork loop in the current epoch",
                registry
            ).unwrap(),
        }
        .pipe(Arc::new)
    }
}
