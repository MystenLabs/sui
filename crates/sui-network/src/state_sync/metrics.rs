// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_metrics::histogram::Histogram as MystenHistogram;
use prometheus::{
    Histogram, HistogramVec, IntCounter, IntCounterVec, IntGauge, Registry,
    register_histogram_vec_with_registry, register_histogram_with_registry,
    register_int_counter_vec_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry,
};
use std::sync::Arc;
use std::time::Duration;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tap::Pipe;

/// Buckets for checkpoint contents download latencies. Requests to nearby peers finish in tens
/// of milliseconds and requests to distant peers in one round trip (up to ~300 ms); the finer
/// buckets up to 1 s separate one-round-trip responses from responses that needed several.
const CHECKPOINT_CONTENT_LATENCY_SEC_BUCKETS: &[f64] = &[
    0.025, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 5.0, 10.0, 30.0,
];

/// Buckets for hedge timing: how long after a download's first request each peer has the
/// checkpoint, and shortened hedge delays. The 25 ms steps between 50 and 300 ms resolve the range
/// in which distant peers answer and hedges are sent. The first bucket holds peers that already
/// had the checkpoint when the first request went out.
const HEDGE_TIMING_SEC_BUCKETS: &[f64] = &[
    0.005, 0.025, 0.05, 0.075, 0.1, 0.125, 0.15, 0.175, 0.2, 0.225, 0.25, 0.275, 0.3, 0.35, 0.4,
    0.5, 0.75, 1.0, 2.0, 5.0,
];

/// How a single checkpoint contents request to one peer ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContentRequestOutcome {
    /// The peer returned contents that match the checkpoint's content digest.
    Success,
    /// The request failed or timed out, or the peer returned no contents or contents that do
    /// not match the digest.
    Failure,
    /// The request was dropped because another peer's response for the same checkpoint won.
    Cancelled,
}

impl ContentRequestOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Events of hedged checkpoint contents downloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HedgeEvent {
    /// A hedge request was sent.
    Sent,
    /// A hedge request was sent early because a peer that hadn't been asked yet is expected to
    /// answer sooner than the request in flight. Also counted in `Sent`.
    SentToCloserPeer,
    /// A hedge request was sent early because the request in flight is to a peer that is slow
    /// from this node and a peer that isn't slow and hadn't been asked yet has the checkpoint.
    /// Also counted in `Sent`.
    SentAroundSlowPeer,
    /// The checkpoint's contents came from a hedge request.
    Won,
    /// The hedge delay passed, but no peer that hadn't been asked yet had announced the
    /// checkpoint. Counted at most once per download attempt.
    NoCandidate,
}

impl HedgeEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::SentToCloserPeer => "sent_to_closer_peer",
            Self::SentAroundSlowPeer => "sent_around_slow_peer",
            Self::Won => "won",
            Self::NoCandidate => "no_candidate",
        }
    }
}

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

    pub fn set_highest_known_checkpoint(&self, sequence_number: CheckpointSequenceNumber) {
        if let Some(inner) = &self.0 {
            inner.highest_known_checkpoint.set(sequence_number as i64);
        }
    }

    pub fn set_highest_verified_checkpoint(&self, sequence_number: CheckpointSequenceNumber) {
        if let Some(inner) = &self.0 {
            inner
                .highest_verified_checkpoint
                .set(sequence_number as i64);
        }
    }

    pub fn set_highest_synced_checkpoint(&self, sequence_number: CheckpointSequenceNumber) {
        if let Some(inner) = &self.0 {
            inner.highest_synced_checkpoint.set(sequence_number as i64);
        }
    }

    pub fn update_checkpoints_synced_from_archive(&self) {
        if let Some(inner) = &self.0 {
            inner.checkpoints_synced_from_archive.inc();
        }
    }

    pub fn checkpoint_summary_age_metrics(&self) -> Option<(&Histogram, &MystenHistogram)> {
        if let Some(inner) = &self.0 {
            return Some((
                &inner.checkpoint_summary_age,
                &inner.checkpoint_summary_age_ms,
            ));
        }
        None
    }

    pub fn inc_peers_reported_for_failure(&self) {
        if let Some(inner) = &self.0 {
            inner.peers_reported_for_failure.inc();
        }
    }

    /// Records one finished checkpoint contents request to `peer`. Only successful requests
    /// enter the response latency histogram; failed and cancelled ones are only counted.
    pub fn record_checkpoint_content_peer_request(
        &self,
        peer: &str,
        outcome: ContentRequestOutcome,
        elapsed: Duration,
    ) {
        if let Some(inner) = &self.0 {
            inner
                .checkpoint_content_peer_requests
                .with_label_values(&[peer, outcome.as_str()])
                .inc();
            if outcome == ContentRequestOutcome::Success {
                inner
                    .checkpoint_content_peer_response_latency
                    .with_label_values(&[peer])
                    .observe(elapsed.as_secs_f64());
            }
        }
    }

    /// Records one download attempt of a checkpoint's contents: the time from its first request
    /// to the first valid response, across all peers asked.
    pub fn record_checkpoint_content_download(&self, elapsed: Duration) {
        if let Some(inner) = &self.0 {
            inner
                .checkpoint_content_download_latency
                .observe(elapsed.as_secs_f64());
        }
    }

    pub fn inc_checkpoint_content_hedge(&self, event: HedgeEvent) {
        if let Some(inner) = &self.0 {
            inner
                .checkpoint_content_hedges
                .with_label_values(&[event.as_str()])
                .inc();
        }
    }

    pub fn inc_checkpoint_content_fallback_requests(&self) {
        if let Some(inner) = &self.0 {
            inner.checkpoint_content_fallback_requests.inc();
        }
    }

    /// Records a request to `peer`, for a checkpoint in the hedge window, that is due to be hedged
    /// after the peer's slow-response threshold, `delay`, instead of after the configured hedge
    /// delay.
    pub fn record_checkpoint_content_slow_peer_hedge_delay(&self, peer: &str, delay: Duration) {
        if let Some(inner) = &self.0 {
            inner
                .checkpoint_content_slow_peer_hedge_delay
                .with_label_values(&[peer])
                .observe(delay.as_secs_f64());
        }
    }

    /// Counts a checkpoint contents request to `peer`, which is slow from this node, that is a
    /// slow-peer probe.
    pub fn inc_checkpoint_content_slow_peer_probe(&self, peer: &str) {
        if let Some(inner) = &self.0 {
            inner
                .checkpoint_content_slow_peer_probes
                .with_label_values(&[peer])
                .inc();
        }
    }

    /// Records how long after a download's first request a poll first saw `peer` having the
    /// checkpoint.
    pub fn record_checkpoint_content_peer_announce_delay(&self, peer: &str, delay: Duration) {
        if let Some(inner) = &self.0 {
            inner
                .checkpoint_content_peer_announce_delay
                .with_label_values(&[peer])
                .observe(delay.as_secs_f64());
        }
    }

    #[cfg(test)]
    pub fn checkpoint_content_slow_peer_hedge_delays(&self, peer: &str) -> u64 {
        self.0
            .as_ref()
            .map(|inner| {
                inner
                    .checkpoint_content_slow_peer_hedge_delay
                    .with_label_values(&[peer])
                    .get_sample_count()
            })
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub fn checkpoint_content_slow_peer_probes(&self, peer: &str) -> u64 {
        self.0
            .as_ref()
            .map(|inner| {
                inner
                    .checkpoint_content_slow_peer_probes
                    .with_label_values(&[peer])
                    .get()
            })
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub fn checkpoint_content_peer_announce_delays(&self, peer: &str) -> u64 {
        self.0
            .as_ref()
            .map(|inner| {
                inner
                    .checkpoint_content_peer_announce_delay
                    .with_label_values(&[peer])
                    .get_sample_count()
            })
            .unwrap_or(0)
    }

    /// How many of `peer`'s announce delays are at most `upper_bound_secs`, which must be one
    /// of the histogram's bucket bounds.
    #[cfg(test)]
    pub fn checkpoint_content_peer_announce_delays_within(
        &self,
        peer: &str,
        upper_bound_secs: f64,
    ) -> u64 {
        use prometheus::core::Metric;
        self.0
            .as_ref()
            .map(|inner| {
                inner
                    .checkpoint_content_peer_announce_delay
                    .with_label_values(&[peer])
                    .metric()
                    .get_histogram()
                    .get_bucket()
                    .iter()
                    .find(|bucket| bucket.upper_bound() == upper_bound_secs)
                    .expect("upper_bound_secs should be a bucket bound")
                    .cumulative_count()
            })
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub fn checkpoint_content_hedges(&self, event: HedgeEvent) -> u64 {
        self.0
            .as_ref()
            .map(|inner| {
                inner
                    .checkpoint_content_hedges
                    .with_label_values(&[event.as_str()])
                    .get()
            })
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub fn checkpoint_content_fallback_requests(&self) -> u64 {
        self.0
            .as_ref()
            .map(|inner| inner.checkpoint_content_fallback_requests.get())
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub fn checkpoint_content_peer_requests(
        &self,
        peer: &str,
        outcome: ContentRequestOutcome,
    ) -> u64 {
        self.0
            .as_ref()
            .map(|inner| {
                inner
                    .checkpoint_content_peer_requests
                    .with_label_values(&[peer, outcome.as_str()])
                    .get()
            })
            .unwrap_or(0)
    }

    pub fn get_peers_reported_for_failure(&self) -> u64 {
        self.0
            .as_ref()
            .map(|inner| inner.peers_reported_for_failure.get())
            .unwrap_or(0)
    }
}

struct Inner {
    highest_known_checkpoint: IntGauge,
    highest_verified_checkpoint: IntGauge,
    highest_synced_checkpoint: IntGauge,
    checkpoints_synced_from_archive: IntCounter,
    peers_reported_for_failure: IntCounter,
    checkpoint_summary_age: Histogram,
    // TODO: delete once users are migrated to non-Mysten histogram.
    checkpoint_summary_age_ms: MystenHistogram,
    checkpoint_content_peer_response_latency: HistogramVec,
    checkpoint_content_peer_requests: IntCounterVec,
    checkpoint_content_download_latency: Histogram,
    checkpoint_content_hedges: IntCounterVec,
    checkpoint_content_fallback_requests: IntCounter,
    checkpoint_content_slow_peer_hedge_delay: HistogramVec,
    checkpoint_content_slow_peer_probes: IntCounterVec,
    checkpoint_content_peer_announce_delay: HistogramVec,
}

impl Inner {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Self {
            highest_known_checkpoint: register_int_gauge_with_registry!(
                "highest_known_checkpoint",
                "Highest known checkpoint",
                registry
            )
            .unwrap(),

            highest_verified_checkpoint: register_int_gauge_with_registry!(
                "highest_verified_checkpoint",
                "Highest verified checkpoint",
                registry
            )
            .unwrap(),

            highest_synced_checkpoint: register_int_gauge_with_registry!(
                "highest_synced_checkpoint",
                "Highest synced checkpoint",
                registry
            )
            .unwrap(),
            checkpoints_synced_from_archive: register_int_counter_with_registry!(
                "checkpoints_synced_from_archive",
                "Checkpoints synced from archive",
                registry
            )
            .unwrap(),
            peers_reported_for_failure: register_int_counter_with_registry!(
                "state_sync_peers_reported_for_failure",
                "Peers reported due to consistent state sync failures",
                registry
            )
            .unwrap(),
            checkpoint_summary_age: register_histogram_with_registry!(
                "checkpoint_summary_age",
                "Age of checkpoints summaries when they arrive and are verified.",
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_summary_age_ms: MystenHistogram::new_in_registry(
                "checkpoint_summary_age_ms",
                "Age of checkpoints summaries when they arrive and are verified.",
                registry,
            ),
            checkpoint_content_peer_response_latency: register_histogram_vec_with_registry!(
                "state_sync_checkpoint_content_peer_response_latency",
                "Time for one peer to answer one checkpoint contents request, for successful \
                 requests. Only peers in the network's known-peer set (seed and allowlisted \
                 peers, and committee members on validators) get their own label; all others \
                 share the `other` label.",
                &["peer_id"],
                CHECKPOINT_CONTENT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_content_peer_requests: register_int_counter_vec_with_registry!(
                "state_sync_checkpoint_content_peer_requests",
                "Finished checkpoint contents requests by peer and outcome (success, failure, \
                 cancelled).",
                &["peer_id", "outcome"],
                registry,
            )
            .unwrap(),
            checkpoint_content_download_latency: register_histogram_with_registry!(
                "state_sync_checkpoint_content_download_latency",
                "Time to get one checkpoint's contents in one download attempt: from the first \
                 request to the first valid response, across all peers asked. With hedging on, \
                 this includes time spent waiting for another peer to announce the checkpoint. \
                 After every peer fails, the retry is a new attempt and starts a new measurement.",
                CHECKPOINT_CONTENT_LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_content_hedges: register_int_counter_vec_with_registry!(
                "state_sync_checkpoint_content_hedges",
                "Hedged checkpoint contents download events: sent, sent_to_closer_peer (sent \
                 early because a peer that hadn't been asked yet is expected to answer sooner \
                 than the request in flight; also counted in sent), sent_around_slow_peer (sent \
                 early because the request in flight is to a peer that is slow from this node \
                 and a peer that isn't slow and hadn't been asked yet has the checkpoint; also \
                 counted in sent), won, and no_candidate (the hedge delay passed with no other \
                 peer having the checkpoint; at most once per download attempt).",
                &["event"],
                registry,
            )
            .unwrap(),
            checkpoint_content_fallback_requests: register_int_counter_with_registry!(
                "state_sync_checkpoint_content_fallback_requests",
                "Checkpoint contents requests sent in a download attempt after every earlier \
                 request in that attempt failed.",
                registry,
            )
            .unwrap(),
            checkpoint_content_slow_peer_hedge_delay: register_histogram_vec_with_registry!(
                "state_sync_checkpoint_content_slow_peer_hedge_delay",
                "The shortened hedge delay of each checkpoint contents request to a peer that \
                 is slow from this node, for checkpoints in the hedge window, by peer. Same peer \
                 labels as state_sync_checkpoint_content_peer_requests.",
                &["peer_id"],
                HEDGE_TIMING_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            checkpoint_content_slow_peer_probes: register_int_counter_vec_with_registry!(
                "state_sync_checkpoint_content_slow_peer_probes",
                "Checkpoint contents requests to a peer that is slow from this node that are \
                 slow-peer probes, by peer: one in four requests that are not hedges, which are \
                 not hedged right away just because the peer is slow, so that they measure \
                 whether it still is. Same peer labels as \
                 state_sync_checkpoint_content_peer_requests.",
                &["peer_id"],
                registry,
            )
            .unwrap(),
            checkpoint_content_peer_announce_delay: register_histogram_vec_with_registry!(
                "state_sync_checkpoint_content_peer_announce_delay",
                "For hedged downloads near the release frontier, by peer: the time from the \
                 download's first request until a poll (every 10 ms) first saw the peer having \
                 the checkpoint (connected, on the same chain, and still holding its contents). \
                 Peers that already had it when the first request went out, including the peer \
                 asked first, record about zero. Peers that announce it after the download ends \
                 are not recorded. Same peer labels as \
                 state_sync_checkpoint_content_peer_requests.",
                &["peer_id"],
                HEDGE_TIMING_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
        }
        .pipe(Arc::new)
    }
}
