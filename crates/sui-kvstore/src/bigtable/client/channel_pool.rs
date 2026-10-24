// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pool of BigTable channels, each one HTTP/2 connection.
//!
//! Each request goes to the non-quarantined channel with the lowest load: its requests in flight,
//! plus a penalty of `max_rpcs_per_channel` for a few seconds after a request on it fails with
//! `Unavailable`, `ResourceExhausted`, `Internal`, or a connection error, or a ReadRows on it is
//! abandoned after its gap timeout. A channel whose requests fail fast never builds up requests
//! in flight; the penalty makes it look a full channel's worth busier, so it gets traffic only
//! once every other channel is that much busier. A request stays in flight until its response
//! body ends or is dropped, and every response frame it receives counts as progress for its
//! channel.
//!
//! A health checker looks for channels where callers are waiting but nothing has arrived for a
//! while, although another channel is receiving responses. A caller waits from its request until
//! the response headers arrive, and again from each body poll that finds no frame buffered until
//! its next poll or until it drops the response. A caller busy with data it already has doesn't
//! count, so a slow consumer does not by itself make its channel look stalled; its unread data
//! still holds the connection's flow-control window, though, and can starve the waiting callers
//! that do count. The checker confirms each suspect with a `PingAndWarm` probe on that channel;
//! any answer, an error status included, shows the connection works. A channel whose probe times
//! out or fails on the connection is quarantined: its open requests fail at once with a retryable
//! `Unavailable` status (see [`is_quarantine_cancellation`]) without waiting on the stalled
//! connection, and new requests avoid it until a re-probe succeeds. A probe whose credential
//! fetch fails or times out never reaches the connection and leaves its channel as it was.
//! Channels that keep failing probes, or are quarantined far more often than the rest of the
//! pool, are replaced by a freshly connected channel. The health checker never quarantines more
//! than half the pool: when more channels than that look stalled at once, the cause is BigTable
//! or the network, not a channel.

use std::collections::VecDeque;
use std::future::Future;
use std::future::poll_fn;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Weak;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use anyhow::Result;
use arc_swap::ArcSwap;
use bytes::Bytes;
use futures::future::join_all;
use futures::future::try_join_all;
use http::HeaderMap;
use http::Request;
use http::Response;
use http::StatusCode;
use http_body::Frame;
use http_body::SizeHint;
use mysten_common::zip_debug_eq::ZipDebugEqIteratorExt;
use prometheus::HistogramVec;
use prometheus::IntCounter;
use prometheus::IntCounterVec;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use prometheus::register_int_counter_with_registry;
use prometheus::register_int_gauge_with_registry;
use rand::Rng;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio::time::MissedTickBehavior;
use tonic::Code;
use tonic::Status;
use tonic::body::Body;
use tonic::codegen::Service;
use tonic::transport::Channel;
use tonic::transport::Endpoint;
use tracing::Level;
use tracing::info;
use tracing::warn;

const MAX_REPLACEMENTS_PER_CYCLE: usize = 2;

/// `PoolEntry::last_response_nanos` of a channel that has never responded.
const NO_RESPONSE: u64 = u64::MAX;

/// How long a channel carries an error penalty in selection after a request on it failed in a
/// way that points at the channel or what serves it. Matches the Go client.
const ERROR_PENALTY_DURATION: Duration = Duration::from_secs(5);

/// How long the circuit-breaker condition must stay clear, with responses flowing, before the
/// health checker logs that a pool-wide stall has ended. Keeps a condition that hovers at the
/// threshold from logging on every check.
const CIRCUIT_BREAKER_CLEAR_AFTER: Duration = Duration::from_secs(5);

pub struct PoolConfig {
    pub initial_pool_size: usize,
    pub min_pool_size: usize,
    pub max_pool_size: usize,
    /// Lower end of the per-channel load band: the pool shrinks when the peak in-flight count of
    /// a maintenance interval would leave fewer than this many requests per channel.
    pub min_rpcs_per_channel: usize,
    /// Upper end of the per-channel load band: the pool grows when the peak in-flight count of a
    /// maintenance interval would put more than this many requests on each channel. Selection
    /// also counts a channel carrying an error penalty as this many requests busier.
    pub max_rpcs_per_channel: usize,
    /// Most channels one resize adds or removes.
    pub max_resize_delta: usize,
    pub maintenance_interval: Duration,
    pub refresh_age: Duration,
    pub refresh_jitter: Duration,
    /// Stalled-channel detection and quarantine; `None` disables it. It also needs a channel
    /// primer, which doubles as the health probe.
    pub health_check: Option<ChannelHealthConfig>,
}

/// Thresholds for detecting, quarantining, and replacing stalled channels.
#[derive(Clone, Debug)]
pub struct ChannelHealthConfig {
    /// How often channel progress is inspected.
    pub check_interval: Duration,
    /// A channel needs at least this many requests whose callers are waiting on it to be
    /// suspected of stalling: waiting for response headers, or since a body poll that found no
    /// frame buffered, until the next poll. Callers busy with data they already have don't count.
    /// One caller is enough to warrant a probe: the probe, not a second waiting caller, is what
    /// keeps a request the server is merely slow to answer from quarantining its channel.
    pub suspect_min_waiting_requests: usize,
    /// A channel is suspected once its waiting callers have received no response frame for this
    /// long while some other channel received one within the same span.
    pub suspect_after_silence: Duration,
    /// Deadline for one `PingAndWarm` probe.
    pub probe_timeout: Duration,
    /// Minimum time between probes of one channel, and the re-probe period of quarantined
    /// channels.
    pub probe_interval: Duration,
    /// A quarantined channel still failing probes this long after quarantine is replaced.
    pub replace_after_quarantined_for: Duration,
    /// A channel quarantined at least this many times within `repeat_quarantine_window` is
    /// replaced if it is also an outlier (see `repeat_offender_ratio`).
    pub repeat_quarantine_threshold: usize,
    pub repeat_quarantine_window: Duration,
    /// A repeatedly quarantined channel is an outlier when its quarantine count within the
    /// window is at least this multiple of the other channels' mean count.
    pub repeat_offender_ratio: f64,
    /// Minimum time between health-driven replacements in one pool.
    pub min_replacement_interval: Duration,
}

#[derive(Clone)]
pub(crate) struct ChannelPool {
    inner: Arc<ChannelPoolInner>,
}

pub(crate) struct ChannelPoolInner {
    entries: ArcSwap<Vec<Arc<PoolEntry>>>,
    /// Start offset of the next selection scan; rotating it spreads requests across equally
    /// loaded channels.
    ticker: AtomicUsize,
    load: Arc<PoolLoad>,
    next_channel_id: AtomicU64,
    /// Held while the entry list is rebuilt (refresh, resize, replacement), so concurrent rebuilds
    /// cannot overwrite each other's changes.
    entries_write_lock: tokio::sync::Mutex<()>,
    health_checker: Mutex<HealthCheckerState>,
    /// When an age-based refresh last swapped in a channel; pool creation until the first one.
    last_refresh_at: Mutex<Instant>,
    endpoint: Endpoint,
    config: PoolConfig,
    primer: Option<Box<dyn ChannelPrimer>>,
    metrics: Option<Arc<Metrics>>,
}

pub(crate) struct Metrics {
    pub(crate) pool_size: IntGauge,
    pub(crate) channels_replaced: IntCounter,
    pub(crate) rpcs_completed: IntCounter,
    pub(crate) peak_in_flight: IntGauge,
    pub(crate) quarantined_channels: IntGauge,
    pub(crate) channel_suspicions: IntCounterVec,
    pub(crate) channel_quarantines: IntCounter,
    pub(crate) channel_restores: IntCounter,
    pub(crate) unhealthy_channels_replaced: IntCounterVec,
    pub(crate) requests_cancelled_by_quarantine: IntCounter,
    pub(crate) probe_latency_ms: HistogramVec,
}

/// Identifies one channel of a pool; a replacement channel gets a new id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChannelId(u64);

/// Request extension for retries: steers the request away from channels earlier attempts failed
/// on, and records which channel served it.
#[derive(Clone, Default)]
pub(crate) struct ChannelRouting(Arc<ChannelRoutingInner>);

#[derive(Default)]
struct ChannelRoutingInner {
    avoid: Vec<ChannelId>,
    served_by: OnceLock<(ChannelId, Weak<PoolEntry>)>,
}

/// Pool-wide in-flight request accounting, shared by every entry.
#[derive(Default)]
struct PoolLoad {
    in_flight: AtomicUsize,
    /// Highest `in_flight` since the last resize.
    peak_in_flight: AtomicUsize,
}

// Entries are wrapped in Arc and held by both the pool's entry list and any in-flight
// requests. When an entry is replaced or removed from the pool, in-flight requests keep their
// Arc<PoolEntry> alive until their response ends — no explicit drain needed.
struct PoolEntry {
    id: ChannelId,
    channel: Channel,
    created_at: Instant,
    refresh_at: Instant,
    /// Requests from selection until their response body ends or is dropped.
    in_flight: AtomicUsize,
    /// Requests whose callers are waiting on the channel: for response headers, or for a body
    /// frame after finding none buffered.
    waiting: AtomicUsize,
    /// Start of the channel's current silence, in nanoseconds since `created_at`: the last
    /// response (see `last_response_nanos`), or the moment `waiting` rose from zero if that is
    /// later, so time no caller spent waiting never counts as silence.
    last_progress_nanos: AtomicU64,
    /// When the channel last delivered response headers or a body frame, in nanoseconds since
    /// `created_at`; [`NO_RESPONSE`] until it first does.
    last_response_nanos: AtomicU64,
    /// Until when the channel carries an error penalty in selection, in nanoseconds since
    /// `created_at`; 0 if it never failed that way.
    error_penalty_until_nanos: AtomicU64,
    quarantined: AtomicBool,
    /// Incremented by every quarantine; requests started under an earlier generation are
    /// cancelled.
    quarantine_generation: AtomicU64,
    quarantine_notify: Notify,
    health: Mutex<ChannelHealth>,
    load: Arc<PoolLoad>,
    // Per-channel counters reset each maintenance cycle. Error count triggers a warn log;
    // success count shows how selection spread requests in tests.
    success_count: AtomicUsize,
    error_count: AtomicUsize,
}

#[derive(Default)]
struct ChannelHealth {
    /// Set while the channel is quarantined.
    quarantined_at: Option<Instant>,
    last_probe_at: Option<Instant>,
    /// Start times of recent quarantines, pruned to the repeat-offender window.
    recent_quarantines: VecDeque<Instant>,
}

#[derive(Default)]
struct HealthCheckerState {
    /// Whether the last circuit-breaker log reported a pool-wide stall.
    circuit_breaker_open: bool,
    /// Since when the circuit-breaker condition has been clear while responses flow.
    circuit_breaker_clear_since: Option<Instant>,
    last_replacement_at: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplacementReason {
    StillFailingProbes,
    RepeatOffender,
}

/// One request's share of its channel's and the pool's in-flight counts, and of the channel's
/// waiting count while its caller waits; dropping it ends the request's accounting.
struct InFlightGuard {
    entry: Arc<PoolEntry>,
    /// Whether the request counts toward `entry.waiting`.
    waiting: bool,
}

/// Resolves when the request's channel is quarantined.
type QuarantineSignal = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Response body that keeps its request in flight until the body ends, records every frame as
/// channel progress, counts its caller as waiting while a poll finds no frame, and fails with
/// [`quarantine_cancellation`] as soon as the channel is quarantined.
struct PoolResponseBody {
    inner: Body,
    /// Present until the body ends.
    guard: Option<InFlightGuard>,
    /// The channel's quarantine generation when the request started.
    quarantine_generation: u64,
    /// Registered for wake-up only when a poll finds no frame; frames that are already buffered
    /// are checked against `quarantine_generation` instead.
    quarantined: QuarantineSignal,
    metrics: Option<Arc<Metrics>>,
}

/// Source of the status a request fails with when its channel is quarantined.
#[derive(Debug)]
struct ChannelQuarantined;

/// Channel state captured for health-checker logs.
struct ChannelSnapshot {
    idx: usize,
    id: u64,
    age_ms: u64,
    in_flight: usize,
    waiting: usize,
    silence_ms: u64,
    since_last_refresh_ms: u64,
}

pub(crate) trait ChannelPrimer: Send + Sync + 'static {
    fn prime<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Fetch the credentials [`Self::prime`] sends, so that a probe can get them cached before
    /// its deadline for the connection starts.
    fn fetch_credentials<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

/// What a health probe learned about a channel's connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeOutcome {
    /// BigTable answered, with any status.
    Answered,
    /// No answer within the probe timeout, or the probe failed on the connection.
    NoAnswer,
    /// Fetching credentials failed or timed out, so the probe never reached the connection and
    /// says nothing about it.
    NotSent,
}

impl PoolConfig {
    /// A single-channel pool with no scaling, for testing or local use.
    pub fn singleton() -> Self {
        Self {
            initial_pool_size: 1,
            min_pool_size: 1,
            max_pool_size: 1,
            health_check: None,
            ..Self::default()
        }
    }
}

impl ChannelPool {
    /// Create a pool without connecting or spawning background tasks.
    pub(crate) fn new(
        endpoint: Endpoint,
        config: PoolConfig,
        primer: Option<Box<dyn ChannelPrimer>>,
        registry: Option<&Registry>,
    ) -> Self {
        Self {
            inner: Arc::new(ChannelPoolInner {
                entries: ArcSwap::from_pointee(Vec::new()),
                ticker: AtomicUsize::new(0),
                load: Arc::default(),
                next_channel_id: AtomicU64::new(0),
                entries_write_lock: tokio::sync::Mutex::new(()),
                health_checker: Mutex::default(),
                last_refresh_at: Mutex::new(Instant::now()),
                endpoint,
                config,
                primer,
                metrics: registry.map(|r| Arc::new(Metrics::new(r))),
            }),
        }
    }

    /// Create, connect, and spawn background maintenance tasks.
    pub(crate) async fn new_connected(
        endpoint: Endpoint,
        config: PoolConfig,
        primer: Option<Box<dyn ChannelPrimer>>,
        registry: Option<&Registry>,
    ) -> Result<Self> {
        let pool = Self::new(endpoint, config, primer, registry);
        pool.connect().await?;
        pool.spawn_background_tasks();
        Ok(pool)
    }

    /// Connect and prime `initial_pool_size` channels concurrently.
    pub(crate) async fn connect(&self) -> Result<()> {
        let entries = try_join_all(
            (0..self.inner.config.initial_pool_size).map(|_| self.inner.create_primed_entry()),
        )
        .await?;
        self.inner
            .set_entries(entries.into_iter().map(Arc::new).collect());
        Ok(())
    }

    /// Spawn the maintenance loop, which refreshes aged-out channels and resizes the pool to its
    /// load, and the health checker, which quarantines and replaces stalled channels.
    fn spawn_background_tasks(&self) {
        let weak = Arc::downgrade(&self.inner);
        let period = self.inner.config.maintenance_interval;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(period);
            interval.tick().await; // first tick completes immediately
            loop {
                interval.tick().await;
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                inner.refresh().await;
                inner.resize().await;
            }
        });

        let Some(health) = self.inner.config.health_check.clone() else {
            return;
        };
        if self.inner.primer.is_none() {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(health.check_interval);
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let Some((entry, reason)) = inner.check_channel_health(&health).await else {
                    continue;
                };
                // Connecting the replacement can take a while; keep checking meanwhile.
                let replacing_pool = Arc::downgrade(&inner);
                let probe_timeout = health.probe_timeout;
                tokio::spawn(async move {
                    if let Some(inner) = replacing_pool.upgrade() {
                        inner
                            .replace_unhealthy_channel(&entry, reason, probe_timeout)
                            .await;
                    }
                });
            }
        });
    }
}

impl ChannelPoolInner {
    /// Replace channels that have exceeded their refresh age. Limits replacements
    /// per cycle to `MAX_REPLACEMENTS_PER_CYCLE` to avoid reconnection storms.
    /// Also resets and logs per-channel error/success counters.
    async fn refresh(&self) {
        let _write = self.entries_write_lock.lock().await;
        let snapshot = self.entries.load_full();
        let mut new_entries: Vec<Arc<PoolEntry>> = (*snapshot).clone();
        let now = Instant::now();
        let mut replacements = 0;

        for (idx, entry) in snapshot.iter().enumerate() {
            let successes = entry.success_count.swap(0, Ordering::Relaxed);
            let errors = entry.error_count.swap(0, Ordering::Relaxed);
            if errors > 0 {
                warn!(
                    channel_idx = idx,
                    errors, successes, "channel errors since last maintenance cycle"
                );
            }

            if replacements >= MAX_REPLACEMENTS_PER_CYCLE {
                continue;
            }

            if now < entry.refresh_at {
                continue;
            }

            match self.create_primed_entry().await {
                Ok(new_entry) => {
                    info!(channel_idx = idx, "replacing bigtable channel");
                    new_entries[idx] = Arc::new(new_entry);
                    replacements += 1;
                }
                Err(e) => {
                    warn!(channel_idx = idx, error = %e, "failed to create replacement channel");
                }
            }
        }

        if replacements > 0 {
            *self.last_refresh_at.lock().unwrap() = Instant::now();
            self.set_entries(new_entries);
            if let Some(m) = &self.metrics {
                m.channels_replaced.inc_by(replacements as u64);
            }
        }
    }

    /// Resize the pool toward the band of sizes that keeps the interval's peak in-flight count
    /// between `min_rpcs_per_channel` and `max_rpcs_per_channel` per channel, by at most
    /// `max_resize_delta` channels, never below `min_pool_size`. Inside the band the size is left
    /// alone, so load that moves within it causes no churn. Shrinking drops quarantined channels
    /// first.
    async fn resize(&self) {
        let _write = self.entries_write_lock.lock().await;
        let entries = self.entries.load_full();
        let current_size = entries.len();
        let peak_in_flight = self.load.take_peak();
        if let Some(m) = &self.metrics {
            m.peak_in_flight.set(peak_in_flight as i64);
        }
        if current_size == 0 {
            return;
        }
        let Some(target_size) = resize_target(peak_in_flight, current_size, &self.config) else {
            return;
        };

        let mut new_entries = (*entries).clone();
        if target_size > current_size {
            let created =
                join_all((current_size..target_size).map(|_| self.create_primed_entry())).await;
            for entry in created {
                match entry {
                    Ok(entry) => new_entries.push(Arc::new(entry)),
                    Err(e) => warn!(error = %e, "failed to create channel during pool expansion"),
                }
            }
            info!(
                current_size,
                new_size = new_entries.len(),
                peak_in_flight,
                "pool resize: expanded"
            );
        } else {
            let mut excess = current_size - target_size;
            new_entries.retain(|entry| {
                let remove = excess > 0 && entry.is_quarantined();
                excess -= usize::from(remove);
                !remove
            });
            new_entries.truncate(new_entries.len() - excess);
            info!(
                current_size,
                new_size = new_entries.len(),
                peak_in_flight,
                "pool resize: shrunk"
            );
        }
        self.set_entries(new_entries);
    }

    /// Inspect every channel once: probe channels that look stalled and quarantined channels due
    /// for a re-probe, quarantine suspects whose probe fails, and restore quarantined channels
    /// whose probe succeeds. Returns the channel to replace, if one is due and the pool-wide
    /// replacement rate limit allows it.
    async fn check_channel_health(
        &self,
        health: &ChannelHealthConfig,
    ) -> Option<(Arc<PoolEntry>, ReplacementReason)> {
        let entries = self.entries.load_full();
        let now = Instant::now();
        let quarantined = entries.iter().filter(|e| e.is_quarantined()).count();
        let stalled: Vec<usize> = (0..entries.len())
            .filter(|&idx| {
                let entry = &entries[idx];
                !entry.is_quarantined()
                    && entry.waiting.load(Ordering::Relaxed) >= health.suspect_min_waiting_requests
                    && entry.silence(now) >= health.suspect_after_silence
            })
            .collect();
        // Silence only implicates a channel when another channel is still getting responses;
        // when every channel is silent, BigTable or the network is the cause. Only responses
        // count here: a probe answer says nothing about whether data is flowing.
        let any_progressing = entries.iter().any(|e| {
            !e.is_quarantined() && e.since_last_response(now) < health.suspect_after_silence
        });
        // Stalls on more than half the pool point at BigTable or the network too.
        let pool_wide_stall =
            !stalled.is_empty() && quarantined + stalled.len() > entries.len() / 2;
        self.log_circuit_breaker(
            pool_wide_stall,
            any_progressing,
            now,
            stalled.len(),
            quarantined,
            entries.len(),
        );

        // (entry index, whether the entry is a suspect rather than a quarantined channel)
        let mut probes: Vec<(usize, bool)> = Vec::new();
        if any_progressing {
            for idx in stalled {
                if !entries[idx].start_probe(now, health.probe_interval) {
                    continue;
                }
                if pool_wide_stall {
                    self.count_suspicion("circuit_breaker_skipped");
                    continue;
                }
                probes.push((idx, true));
            }
        }
        for (idx, entry) in entries.iter().enumerate() {
            if entry.is_quarantined() && entry.start_probe(now, health.probe_interval) {
                probes.push((idx, false));
            }
        }
        let probe_results = join_all(
            probes
                .iter()
                .map(|&(idx, _)| self.probe(&entries[idx], health.probe_timeout)),
        )
        .await;

        // Probing can take a while, during which a resize may have changed the pool; cap
        // quarantines against the pool as it is now.
        let now = Instant::now();
        let live_entries = self.entries.load_full();
        let max_quarantined = live_entries.len() / 2;
        let mut quarantined = live_entries.iter().filter(|e| e.is_quarantined()).count();
        let mut replacement = None;
        for (&(idx, suspected), outcome) in probes.iter().zip_debug_eq(probe_results) {
            let entry = &entries[idx];
            if outcome == ProbeOutcome::NotSent {
                // Leave the channel as it is; the next probe that gets through decides.
                if suspected {
                    self.count_suspicion("probe_not_sent");
                    log_channel_event(
                        Level::WARN,
                        "bigtable channel looked stalled, but its probe could not fetch credentials; leaving it in service",
                        &self.snapshot(idx, entry, now),
                    );
                }
                continue;
            }
            let probe_ok = outcome == ProbeOutcome::Answered;
            if !live_entries.iter().any(|live| Arc::ptr_eq(live, entry)) {
                // A resize or refresh dropped the channel while it was probed. Outside the pool
                // it neither counts toward the cap nor needs replacing, but a failed probe still
                // fails the requests left on it.
                if suspected && !probe_ok {
                    self.quarantine(entry, now);
                }
                continue;
            }
            match (suspected, probe_ok) {
                (true, true) => {
                    log_channel_event(
                        Level::DEBUG,
                        "bigtable channel looked stalled but answered its probe; leaving it in service",
                        &self.snapshot(idx, entry, now),
                    );
                    entry.restart_silence();
                    self.count_suspicion("probe_ok");
                }
                (true, false) if quarantined < max_quarantined => {
                    let snapshot = self.snapshot(idx, entry, now);
                    self.quarantine(entry, now);
                    quarantined += 1;
                    self.count_suspicion("quarantined");
                    log_channel_event(
                        Level::WARN,
                        "bigtable channel looked stalled and failed its probe; quarantined it and cancelled its open requests",
                        &snapshot,
                    );
                    if replacement.is_none() && is_repeat_offender(&entries, idx, now, health) {
                        replacement = Some((entry.clone(), ReplacementReason::RepeatOffender));
                    }
                }
                (true, false) => {
                    self.count_suspicion("circuit_breaker_skipped");
                    log_channel_event(
                        Level::WARN,
                        "bigtable channel looked stalled and failed its probe, but half the pool is already quarantined; leaving it in service",
                        &self.snapshot(idx, entry, now),
                    );
                }
                (false, true) => {
                    self.restore(entry);
                    quarantined = quarantined.saturating_sub(1);
                    log_channel_event(
                        Level::WARN,
                        "quarantined bigtable channel passed its probe; restored it",
                        &self.snapshot(idx, entry, now),
                    );
                }
                (false, false) => {
                    if replacement.is_none()
                        && entry.quarantined_for(now) >= health.replace_after_quarantined_for
                    {
                        replacement = Some((entry.clone(), ReplacementReason::StillFailingProbes));
                    }
                }
            }
        }
        self.update_quarantined_gauge(&live_entries);

        let replacement = replacement.filter(|_| !pool_wide_stall)?;
        let mut checker = self.health_checker.lock().unwrap();
        if checker
            .last_replacement_at
            .is_some_and(|at| now.saturating_duration_since(at) < health.min_replacement_interval)
        {
            return None;
        }
        checker.last_replacement_at = Some(now);
        Some(replacement)
    }

    /// Swap a newly connected channel in for `entry`, once the new channel has passed a probe.
    /// The old channel leaves the pool only when its replacement has answered, so the pool never
    /// shrinks; if the replacement cannot connect or fails its probe, the old channel stays.
    /// Returns whether the swap happened.
    async fn replace_unhealthy_channel(
        &self,
        entry: &Arc<PoolEntry>,
        reason: ReplacementReason,
        probe_timeout: Duration,
    ) -> bool {
        if !self.entries.load().iter().any(|e| Arc::ptr_eq(e, entry)) {
            return false;
        }
        let replacement = match self.connect_entry().await {
            Ok(replacement) => Arc::new(replacement),
            Err(e) => {
                warn!(
                    channel_id = entry.id.0,
                    reason = reason.label(),
                    error = %e,
                    "failed to connect a replacement for an unhealthy bigtable channel; keeping it"
                );
                return false;
            }
        };
        if self.probe(&replacement, probe_timeout).await != ProbeOutcome::Answered {
            warn!(
                channel_id = entry.id.0,
                reason = reason.label(),
                "replacement for an unhealthy bigtable channel failed its probe; keeping the old channel"
            );
            return false;
        }

        let _write = self.entries_write_lock.lock().await;
        let mut new_entries = (**self.entries.load()).clone();
        let Some(idx) = new_entries.iter().position(|e| Arc::ptr_eq(e, entry)) else {
            return false;
        };
        let snapshot = self.snapshot(idx, entry, Instant::now());
        new_entries[idx] = replacement;
        self.set_entries(new_entries);
        if let Some(m) = &self.metrics {
            m.unhealthy_channels_replaced
                .with_label_values(&[reason.label()])
                .inc();
        }
        log_channel_event(
            Level::WARN,
            &format!("replaced unhealthy bigtable channel ({})", reason.label()),
            &snapshot,
        );
        true
    }

    /// The non-quarantined channel with the lowest load, preferring channels the request's
    /// routing does not avoid. Load is requests in flight, plus `max_rpcs_per_channel` while the
    /// channel carries an error penalty. Go's flat penalty of 10 would still let a channel that
    /// fails fast win most requests once the others each have more than 10 in flight, which the
    /// load band allows. Ties go to the first channel from a rotating start, so equally loaded
    /// channels share requests.
    fn select_channel(&self, routing: Option<&ChannelRouting>) -> Option<Arc<PoolEntry>> {
        let entries = self.entries.load();
        let start = self.ticker.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();
        let error_penalty_load = self.config.max_rpcs_per_channel;
        (0..entries.len())
            .map(|offset| &entries[(start + offset) % entries.len()])
            .min_by_key(|entry| {
                let penalty = if entry.is_penalized(now) {
                    error_penalty_load
                } else {
                    0
                };
                (
                    entry.is_quarantined(),
                    routing.is_some_and(|routing| routing.avoids(entry.id)),
                    entry.in_flight.load(Ordering::Relaxed) + penalty,
                )
            })
            .cloned()
    }

    /// Send a `PingAndWarm` on `entry`'s channel and report whether the connection answered within
    /// `timeout`. Any status BigTable sends back counts as an answer, errors included: the probe
    /// tests the connection, and an error such as a missing permission still made the round trip.
    /// Credentials are fetched first, under a deadline of their own: a token fetch that fails or
    /// is slow says nothing about the connection.
    async fn probe(&self, entry: &PoolEntry, timeout: Duration) -> ProbeOutcome {
        let Some(primer) = self.primer.as_deref() else {
            return ProbeOutcome::NoAnswer;
        };
        let started_at = Instant::now();
        let outcome = match tokio::time::timeout(timeout, primer.fetch_credentials()).await {
            Ok(Ok(())) => match tokio::time::timeout(timeout, primer.prime(&entry.channel)).await {
                Ok(Ok(())) => ProbeOutcome::Answered,
                Ok(Err(e)) if is_status_from_server(&e) => ProbeOutcome::Answered,
                Ok(Err(_)) | Err(_) => ProbeOutcome::NoAnswer,
            },
            Ok(Err(_)) | Err(_) => ProbeOutcome::NotSent,
        };
        if let Some(m) = &self.metrics {
            let label = match outcome {
                ProbeOutcome::Answered => "ok",
                ProbeOutcome::NoAnswer => "failed",
                ProbeOutcome::NotSent => "not_sent",
            };
            m.probe_latency_ms
                .with_label_values(&[label])
                .observe(started_at.elapsed().as_secs_f64() * 1000.0);
        }
        outcome
    }

    /// Take `entry` out of selection and fail its open requests.
    fn quarantine(&self, entry: &PoolEntry, now: Instant) {
        {
            let mut health = entry.health.lock().unwrap();
            health.quarantined_at = Some(now);
            health.recent_quarantines.push_back(now);
        }
        entry.quarantined.store(true, Ordering::SeqCst);
        entry.quarantine_generation.fetch_add(1, Ordering::SeqCst);
        entry.quarantine_notify.notify_waiters();
        if let Some(m) = &self.metrics {
            m.channel_quarantines.inc();
        }
    }

    /// Return a quarantined `entry` to selection.
    fn restore(&self, entry: &PoolEntry) {
        entry.health.lock().unwrap().quarantined_at = None;
        entry.restart_silence();
        entry.quarantined.store(false, Ordering::SeqCst);
        if let Some(m) = &self.metrics {
            m.channel_restores.inc();
        }
    }

    /// Log when a pool-wide stall begins, and when the condition has been clear with responses
    /// flowing for [`CIRCUIT_BREAKER_CLEAR_AFTER`]. A pool where nothing responds has not
    /// recovered, so it never logs the end of a stall.
    fn log_circuit_breaker(
        &self,
        pool_wide_stall: bool,
        any_progressing: bool,
        now: Instant,
        stalled: usize,
        quarantined: usize,
        pool_size: usize,
    ) {
        let mut checker = self.health_checker.lock().unwrap();
        if pool_wide_stall {
            checker.circuit_breaker_clear_since = None;
            if !checker.circuit_breaker_open {
                checker.circuit_breaker_open = true;
                warn!(
                    stalled,
                    quarantined,
                    pool_size,
                    "quarantined or stalled channels make up more than half the bigtable pool; BigTable or the network is the likely cause, so no channel is quarantined or replaced"
                );
            }
            return;
        }
        if !checker.circuit_breaker_open {
            return;
        }
        if !any_progressing {
            checker.circuit_breaker_clear_since = None;
            return;
        }
        let clear_since = *checker.circuit_breaker_clear_since.get_or_insert(now);
        if now.saturating_duration_since(clear_since) >= CIRCUIT_BREAKER_CLEAR_AFTER {
            checker.circuit_breaker_open = false;
            checker.circuit_breaker_clear_since = None;
            info!(
                pool_size,
                "bigtable channel stalls no longer span most of the pool"
            );
        }
    }

    fn count_suspicion(&self, outcome: &str) {
        if let Some(m) = &self.metrics {
            m.channel_suspicions.with_label_values(&[outcome]).inc();
        }
    }

    fn snapshot(&self, idx: usize, entry: &PoolEntry, now: Instant) -> ChannelSnapshot {
        let last_refresh_at = *self.last_refresh_at.lock().unwrap();
        ChannelSnapshot {
            idx,
            id: entry.id.0,
            age_ms: millis(now.saturating_duration_since(entry.created_at)),
            in_flight: entry.in_flight.load(Ordering::Relaxed),
            waiting: entry.waiting.load(Ordering::Relaxed),
            silence_ms: millis(entry.silence(now)),
            since_last_refresh_ms: millis(now.saturating_duration_since(last_refresh_at)),
        }
    }

    fn set_entries(&self, entries: Vec<Arc<PoolEntry>>) {
        if let Some(m) = &self.metrics {
            m.pool_size.set(entries.len() as i64);
        }
        self.update_quarantined_gauge(&entries);
        self.entries.store(Arc::new(entries));
    }

    fn update_quarantined_gauge(&self, entries: &[Arc<PoolEntry>]) {
        if let Some(m) = &self.metrics {
            let quarantined = entries.iter().filter(|e| e.is_quarantined()).count();
            m.quarantined_channels.set(quarantined as i64);
        }
    }

    /// Connect and prime a channel. A failed prime is logged and the channel is used anyway.
    async fn create_primed_entry(&self) -> Result<PoolEntry> {
        let entry = self.connect_entry().await?;
        if let Some(primer) = self.primer.as_deref()
            && let Err(e) = primer.prime(&entry.channel).await
        {
            warn!(error = %e, "channel priming failed (non-fatal)");
        }
        Ok(entry)
    }

    async fn connect_entry(&self) -> Result<PoolEntry> {
        let channel = self.endpoint.connect().await?;
        Ok(PoolEntry {
            id: ChannelId(self.next_channel_id.fetch_add(1, Ordering::Relaxed)),
            channel,
            created_at: Instant::now(),
            refresh_at: compute_refresh_at(self.config.refresh_age, self.config.refresh_jitter),
            in_flight: AtomicUsize::new(0),
            waiting: AtomicUsize::new(0),
            last_progress_nanos: AtomicU64::new(0),
            last_response_nanos: AtomicU64::new(NO_RESPONSE),
            error_penalty_until_nanos: AtomicU64::new(0),
            quarantined: AtomicBool::new(false),
            quarantine_generation: AtomicU64::new(0),
            quarantine_notify: Notify::new(),
            health: Mutex::default(),
            load: self.load.clone(),
            success_count: AtomicUsize::new(0),
            error_count: AtomicUsize::new(0),
        })
    }
}

impl PoolEntry {
    fn is_quarantined(&self) -> bool {
        self.quarantined.load(Ordering::SeqCst)
    }

    /// Note response headers or a body frame.
    fn record_response(&self) {
        let nanos = self.nanos_since_created(Instant::now());
        self.last_progress_nanos.store(nanos, Ordering::Relaxed);
        self.last_response_nanos.store(nanos, Ordering::Relaxed);
    }

    /// Start counting silence afresh: a caller started waiting on an idle channel, or the channel
    /// answered a probe.
    fn restart_silence(&self) {
        let nanos = self.nanos_since_created(Instant::now());
        self.last_progress_nanos.store(nanos, Ordering::Relaxed);
    }

    /// Give the channel an error penalty in selection for [`ERROR_PENALTY_DURATION`].
    fn penalize(&self) {
        let until = self.nanos_since_created(Instant::now() + ERROR_PENALTY_DURATION);
        self.error_penalty_until_nanos
            .store(until, Ordering::Relaxed);
    }

    /// Whether the channel carries an error penalty in selection as of `now`.
    fn is_penalized(&self, now: Instant) -> bool {
        self.error_penalty_until_nanos.load(Ordering::Relaxed) > self.nanos_since_created(now)
    }

    /// How long the channel has gone without progress as of `now`.
    fn silence(&self, now: Instant) -> Duration {
        let last_progress_at = self.created_at
            + Duration::from_nanos(self.last_progress_nanos.load(Ordering::Relaxed));
        now.saturating_duration_since(last_progress_at)
    }

    /// How long since the channel's last response as of `now`; `Duration::MAX` if it has never
    /// responded.
    fn since_last_response(&self, now: Instant) -> Duration {
        match self.last_response_nanos.load(Ordering::Relaxed) {
            NO_RESPONSE => Duration::MAX,
            nanos => now.saturating_duration_since(self.created_at + Duration::from_nanos(nanos)),
        }
    }

    fn nanos_since_created(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.created_at).as_nanos() as u64
    }

    /// Claim a probe of this channel at `now`, unless it was probed less than `interval` ago.
    fn start_probe(&self, now: Instant, interval: Duration) -> bool {
        let mut health = self.health.lock().unwrap();
        if health
            .last_probe_at
            .is_some_and(|at| now.saturating_duration_since(at) < interval)
        {
            return false;
        }
        health.last_probe_at = Some(now);
        true
    }

    fn quarantined_for(&self, now: Instant) -> Duration {
        self.health
            .lock()
            .unwrap()
            .quarantined_at
            .map_or(Duration::ZERO, |at| now.saturating_duration_since(at))
    }

    fn recent_quarantine_count(&self, now: Instant, window: Duration) -> usize {
        let mut health = self.health.lock().unwrap();
        while health
            .recent_quarantines
            .front()
            .is_some_and(|&at| now.saturating_duration_since(at) > window)
        {
            health.recent_quarantines.pop_front();
        }
        health.recent_quarantines.len()
    }
}

impl PoolLoad {
    /// The peak since the last call, restarting the next interval's peak at the current count.
    fn take_peak(&self) -> usize {
        self.peak_in_flight
            .swap(self.in_flight.load(Ordering::Relaxed), Ordering::Relaxed)
    }
}

impl ChannelRouting {
    pub(crate) fn avoiding(avoid: Vec<ChannelId>) -> Self {
        Self(Arc::new(ChannelRoutingInner {
            avoid,
            served_by: OnceLock::new(),
        }))
    }

    /// The channel the request was sent on, once the pool has chosen one.
    pub(crate) fn served_by(&self) -> Option<ChannelId> {
        self.0.served_by.get().map(|(id, _)| *id)
    }

    /// Give the channel the request was sent on an error penalty in selection for
    /// [`ERROR_PENALTY_DURATION`], for a failure only the caller sees, such as responses that
    /// stopped arriving.
    pub(crate) fn penalize_serving_channel(&self) {
        if let Some(entry) = self
            .0
            .served_by
            .get()
            .and_then(|(_, entry)| entry.upgrade())
        {
            entry.penalize();
        }
    }

    fn avoids(&self, id: ChannelId) -> bool {
        self.0.avoid.contains(&id)
    }
}

impl ReplacementReason {
    fn label(self) -> &'static str {
        match self {
            Self::StillFailingProbes => "still_failing_probes",
            Self::RepeatOffender => "repeat_offender",
        }
    }
}

impl InFlightGuard {
    fn new(entry: Arc<PoolEntry>) -> Self {
        entry.in_flight.fetch_add(1, Ordering::Relaxed);
        let pool_in_flight = entry.load.in_flight.fetch_add(1, Ordering::Relaxed) + 1;
        entry
            .load
            .peak_in_flight
            .fetch_max(pool_in_flight, Ordering::Relaxed);
        Self {
            entry,
            waiting: false,
        }
    }

    /// Count the request's caller as waiting on the channel, restarting the channel's silence if
    /// no other caller was.
    fn start_waiting(&mut self) {
        if self.waiting {
            return;
        }
        self.waiting = true;
        if self.entry.waiting.fetch_add(1, Ordering::Relaxed) == 0 {
            self.entry.restart_silence();
        }
    }

    fn stop_waiting(&mut self) {
        if self.waiting {
            self.waiting = false;
            self.entry.waiting.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl Metrics {
    fn new(registry: &Registry) -> Self {
        Self {
            pool_size: register_int_gauge_with_registry!(
                "bt_pool_pool_size",
                "Current number of channels in the BigTable connection pool",
                registry,
            )
            .unwrap(),
            channels_replaced: register_int_counter_with_registry!(
                "bt_pool_channels_replaced",
                "Total channels replaced due to age refresh",
                registry,
            )
            .unwrap(),
            rpcs_completed: register_int_counter_with_registry!(
                "bt_pool_rpcs_completed",
                "Total RPCs completed through the pool",
                registry,
            )
            .unwrap(),
            peak_in_flight: register_int_gauge_with_registry!(
                "bt_pool_peak_in_flight",
                "Peak number of in-flight requests (counted until their response body ends) during the last maintenance interval, as used to size the pool",
                registry,
            )
            .unwrap(),
            quarantined_channels: register_int_gauge_with_registry!(
                "bt_pool_quarantined_channels",
                "Channels currently quarantined: taken out of selection after failing a health probe",
                registry,
            )
            .unwrap(),
            channel_suspicions: register_int_counter_vec_with_registry!(
                "bt_pool_channel_suspicions",
                "Channels that held open requests without progress while other channels progressed, by outcome: probe_ok (the PingAndWarm probe succeeded), quarantined (the probe failed), circuit_breaker_skipped (too much of the pool looked stalled to blame the channel), probe_not_sent (the probe could not fetch credentials, so the channel stayed in service)",
                &["outcome"],
                registry,
            )
            .unwrap(),
            channel_quarantines: register_int_counter_with_registry!(
                "bt_pool_channel_quarantines",
                "Channels quarantined after a failed health probe; each quarantine cancels the channel's open requests",
                registry,
            )
            .unwrap(),
            channel_restores: register_int_counter_with_registry!(
                "bt_pool_channel_restores",
                "Quarantined channels returned to service after a successful re-probe",
                registry,
            )
            .unwrap(),
            unhealthy_channels_replaced: register_int_counter_vec_with_registry!(
                "bt_pool_unhealthy_channels_replaced",
                "Quarantined channels replaced with a new connection, by reason: still_failing_probes or repeat_offender",
                &["reason"],
                registry,
            )
            .unwrap(),
            requests_cancelled_by_quarantine: register_int_counter_with_registry!(
                "bt_pool_requests_cancelled_by_quarantine",
                "Open requests failed locally with a retryable Unavailable status because their channel was quarantined",
                registry,
            )
            .unwrap(),
            probe_latency_ms: register_histogram_vec_with_registry!(
                "bt_pool_probe_latency_ms",
                "Latency of PingAndWarm health probes on a specific channel, by outcome (ok, failed, or not_sent; failures include timeouts, not_sent means fetching credentials failed or timed out)",
                &["outcome"],
                prometheus::exponential_buckets(1.0, 1.6, 20)
                    .unwrap()
                    .to_vec(),
                registry,
            )
            .unwrap(),
        }
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            initial_pool_size: 10,
            min_pool_size: 4,
            max_pool_size: 200,
            min_rpcs_per_channel: 1,
            max_rpcs_per_channel: 25,
            max_resize_delta: 2,
            maintenance_interval: Duration::from_secs(60),
            // GFE forcibly disconnects channels after ~60 minutes for load balancing.
            // Refresh at 45m + up to 5m jitter to replace channels before reaping.
            refresh_age: Duration::from_secs(45 * 60),
            refresh_jitter: Duration::from_secs(5 * 60),
            health_check: Some(ChannelHealthConfig::default()),
        }
    }
}

impl Default for ChannelHealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_millis(250),
            suspect_min_waiting_requests: 1,
            suspect_after_silence: Duration::from_secs(1),
            probe_timeout: Duration::from_millis(500),
            probe_interval: Duration::from_secs(1),
            replace_after_quarantined_for: Duration::from_secs(30),
            repeat_quarantine_threshold: 3,
            repeat_quarantine_window: Duration::from_secs(10 * 60),
            repeat_offender_ratio: 3.0,
            min_replacement_interval: Duration::from_secs(60),
        }
    }
}

impl Service<Request<Body>> for ChannelPool {
    type Response = Response<Body>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // The channel is chosen in `call`, which then waits for that channel to be ready.
        if self.inner.entries.load().is_empty() {
            return Poll::Ready(Err("no channels available".into()));
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let routing = request.extensions().get::<ChannelRouting>();
        let Some(entry) = self.inner.select_channel(routing) else {
            return Box::pin(async { Err("no channels available".into()) });
        };
        if let Some(routing) = routing {
            let _ = routing.0.served_by.set((entry.id, Arc::downgrade(&entry)));
        }
        let mut guard = InFlightGuard::new(entry.clone());
        // Read the generation before checking the flag: a quarantine that lands after the check
        // changes the generation and cancels the request below, and one that landed before it is
        // caught by the check.
        let generation = entry.quarantine_generation.load(Ordering::SeqCst);
        let routed_to_quarantined = entry.is_quarantined();
        let metrics = self.inner.metrics.clone();

        Box::pin(async move {
            let mut quarantined: QuarantineSignal =
                Box::pin(quarantined_after(entry.clone(), generation));
            let result = if routed_to_quarantined {
                None
            } else {
                guard.start_waiting();
                let result =
                    send_unless_quarantined(entry.channel.clone(), request, &mut quarantined).await;
                guard.stop_waiting();
                result
            };
            if let Some(m) = &metrics {
                m.rpcs_completed.inc();
            }
            match result {
                None => {
                    entry.error_count.fetch_add(1, Ordering::Relaxed);
                    if let Some(m) = &metrics {
                        m.requests_cancelled_by_quarantine.inc();
                    }
                    Err(quarantine_cancellation().into())
                }
                Some(Err(e)) => {
                    entry.error_count.fetch_add(1, Ordering::Relaxed);
                    entry.penalize();
                    Err(e.into())
                }
                Some(Ok(response)) => {
                    entry.success_count.fetch_add(1, Ordering::Relaxed);
                    entry.record_response();
                    if fails_with_penalized_status(&response) {
                        entry.penalize();
                    }
                    Ok(response.map(|body| {
                        Body::new(PoolResponseBody {
                            inner: body,
                            guard: Some(guard),
                            quarantine_generation: generation,
                            quarantined,
                            metrics,
                        })
                    }))
                }
            }
        })
    }
}

impl PoolResponseBody {
    /// End the body because its channel was quarantined. Dropping the stream only queues its
    /// reset, so nothing waits on the stalled connection.
    fn cancel_for_quarantine(&mut self) -> Status {
        self.inner = Body::empty();
        self.guard = None;
        if let Some(m) = &self.metrics {
            m.requests_cancelled_by_quarantine.inc();
        }
        quarantine_cancellation()
    }
}

impl http_body::Body for PoolResponseBody {
    type Data = Bytes;
    type Error = Status;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Status>>> {
        let this = self.get_mut();
        let Some(guard) = this.guard.as_mut() else {
            return Poll::Ready(None);
        };
        if guard.entry.quarantine_generation.load(Ordering::SeqCst) != this.quarantine_generation {
            return Poll::Ready(Some(Err(this.cancel_for_quarantine())));
        }
        let frame = match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(frame) => frame,
            Poll::Pending => {
                if this.quarantined.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Some(Err(this.cancel_for_quarantine())));
                }
                guard.start_waiting();
                return Poll::Pending;
            }
        };
        guard.stop_waiting();
        match &frame {
            Some(Ok(frame)) => {
                guard.entry.record_response();
                // Trailers end the response and carry its status.
                if let Some(trailers) = frame.trailers_ref() {
                    if carries_penalized_status(trailers) {
                        guard.entry.penalize();
                    }
                    this.guard = None;
                }
            }
            Some(Err(_)) => {
                // The connection failed under the stream.
                guard.entry.penalize();
                this.guard = None;
            }
            None => this.guard = None,
        }
        Poll::Ready(frame)
    }

    fn is_end_stream(&self) -> bool {
        self.guard.is_none() || self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.stop_waiting();
        self.entry.in_flight.fetch_sub(1, Ordering::Relaxed);
        self.entry.load.in_flight.fetch_sub(1, Ordering::Relaxed);
    }
}

impl std::fmt::Display for ChannelQuarantined {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bigtable channel quarantined")
    }
}

impl std::error::Error for ChannelQuarantined {}

/// Whether `status` is the error a request got because its channel was quarantined while the
/// request was open. The request failed locally without waiting on the stalled connection; a read
/// can be retried on another channel, but BigTable may already have applied a write.
pub(crate) fn is_quarantine_cancellation(status: &Status) -> bool {
    status.code() == Code::Unavailable
        && std::error::Error::source(status).is_some_and(|source| source.is::<ChannelQuarantined>())
}

fn quarantine_cancellation() -> Status {
    let mut status = Status::unavailable(ChannelQuarantined.to_string());
    status.set_source(Arc::new(ChannelQuarantined));
    status
}

/// Whether `headers` carry a gRPC status that points at the channel or what serves it, as the Go
/// client judges it: `Unavailable`, `ResourceExhausted`, or `Internal`.
fn carries_penalized_status(headers: &HeaderMap) -> bool {
    headers.get("grpc-status").is_some_and(|status| {
        matches!(
            Code::from_bytes(status.as_bytes()),
            Code::Unavailable | Code::ResourceExhausted | Code::Internal
        )
    })
}

/// Whether `response` fails at once with a status that points at the channel or what serves it.
/// A `grpc-status` header decides. Without one, a non-200 HTTP status fails the request with the
/// code gRPC maps it to: `Internal` for 400, `Unavailable` for 429, 502, 503 and 504.
fn fails_with_penalized_status<B>(response: &Response<B>) -> bool {
    if response.headers().contains_key("grpc-status") {
        return carries_penalized_status(response.headers());
    }
    matches!(
        response.status(),
        StatusCode::BAD_REQUEST
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

/// Whether `error` is a status BigTable sent back, as opposed to one tonic made up for a request
/// that failed on the connection, to which tonic attaches the transport error as the source.
fn is_status_from_server(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<Status>()
        .is_some_and(|status| std::error::Error::source(status).is_none())
}

/// Resolves once `entry` has been quarantined after `generation`.
async fn quarantined_after(entry: Arc<PoolEntry>, generation: u64) {
    loop {
        let notified = entry.quarantine_notify.notified();
        if entry.quarantine_generation.load(Ordering::SeqCst) != generation {
            return;
        }
        notified.await;
    }
}

/// Wait for `channel` to accept a request and send it; `None` if the channel is quarantined
/// before the response headers arrive.
async fn send_unless_quarantined(
    mut channel: Channel,
    request: Request<Body>,
    quarantined: &mut QuarantineSignal,
) -> Option<Result<Response<Body>, tonic::transport::Error>> {
    if let Err(e) = unless_quarantined(quarantined, poll_fn(|cx| channel.poll_ready(cx))).await? {
        return Some(Err(e));
    }
    unless_quarantined(quarantined, channel.call(request)).await
}

async fn unless_quarantined<F: Future>(
    quarantined: &mut QuarantineSignal,
    future: F,
) -> Option<F::Output> {
    let mut future = std::pin::pin!(future);
    poll_fn(|cx| {
        if quarantined.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        future.as_mut().poll(cx).map(Some)
    })
    .await
}

/// Pool size a resize moves to, or `None` when `current_size` already lies in the band of sizes
/// that keep `peak_in_flight` between `min_rpcs_per_channel` and `max_rpcs_per_channel` per
/// channel. Outside the band the size moves toward the band's midpoint by at most
/// `max_resize_delta`.
fn resize_target(peak_in_flight: usize, current_size: usize, config: &PoolConfig) -> Option<usize> {
    let fewest_channels = peak_in_flight
        .div_ceil(config.max_rpcs_per_channel.max(1))
        .max(config.min_pool_size)
        .min(config.max_pool_size);
    let most_channels = match config.min_rpcs_per_channel {
        0 => config.max_pool_size,
        per_channel => peak_in_flight
            .div_ceil(per_channel)
            .min(config.max_pool_size),
    }
    .max(fewest_channels);
    if (fewest_channels..=most_channels).contains(&current_size) {
        return None;
    }
    let midpoint = (fewest_channels + most_channels) / 2;
    let target = if midpoint > current_size {
        midpoint.min(current_size + config.max_resize_delta)
    } else {
        midpoint.max(current_size.saturating_sub(config.max_resize_delta))
    };
    (target != current_size).then_some(target)
}

fn is_repeat_offender(
    entries: &[Arc<PoolEntry>],
    idx: usize,
    now: Instant,
    health: &ChannelHealthConfig,
) -> bool {
    let counts: Vec<usize> = entries
        .iter()
        .map(|entry| entry.recent_quarantine_count(now, health.repeat_quarantine_window))
        .collect();
    is_quarantine_outlier(
        &counts,
        idx,
        health.repeat_quarantine_threshold,
        health.repeat_offender_ratio,
    )
}

/// Whether channel `idx` has at least `threshold` recent quarantines and at least `ratio` times
/// the mean count of the other channels. Comparing against the rest of the pool keeps channels
/// that merely shared a pool-wide bad spell from being replaced.
fn is_quarantine_outlier(counts: &[usize], idx: usize, threshold: usize, ratio: f64) -> bool {
    let own = counts[idx];
    if own < threshold {
        return false;
    }
    let others = counts.len().saturating_sub(1).max(1);
    let others_mean = (counts.iter().sum::<usize>() - own) as f64 / others as f64;
    own as f64 >= ratio * others_mean
}

/// Log a health-checker event about `channel` at `level`.
fn log_channel_event(level: Level, message: &str, channel: &ChannelSnapshot) {
    // tracing fixes each log statement's level at compile time, so each level gets its own.
    macro_rules! log_at {
        ($log:ident) => {
            tracing::$log!(
                channel_idx = channel.idx,
                channel_id = channel.id,
                channel_age_ms = channel.age_ms,
                in_flight = channel.in_flight,
                waiting = channel.waiting,
                silence_ms = channel.silence_ms,
                since_last_refresh_ms = channel.since_last_refresh_ms,
                "{message}"
            )
        };
    }
    match level {
        Level::ERROR => log_at!(error),
        Level::WARN => log_at!(warn),
        Level::INFO => log_at!(info),
        Level::DEBUG => log_at!(debug),
        _ => log_at!(trace),
    }
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis() as u64
}

fn compute_refresh_at(refresh_age: Duration, refresh_jitter: Duration) -> Instant {
    let jitter = if refresh_jitter.is_zero() {
        Duration::ZERO
    } else {
        Duration::from_millis(rand::thread_rng().gen_range(0..refresh_jitter.as_millis() as u64))
    };
    Instant::now() + refresh_age + jitter
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use gcp_auth::Token;
    use gcp_auth::TokenProvider;
    use prometheus::Registry;
    use tokio::sync::mpsc;
    use tonic::Streaming;

    use super::*;
    use crate::bigtable::client::BigTableClient;
    use crate::bigtable::client::BigtablePrimer;
    use crate::bigtable::client::CheckpointSpan;
    use crate::bigtable::client::ReadRowsRetryConfig;
    use crate::bigtable::client::raw_h2_server::RawH2Server;
    use crate::bigtable::client::raw_h2_server::ReadRowsMode;
    use crate::bigtable::metrics::KvMetrics;
    use crate::bigtable::mock_server::MockBigtableServer;
    use crate::bigtable::mock_server::ReadRowsFault;
    use crate::bigtable::mock_server::ReadRowsInterruption;
    use crate::bigtable::proto::bigtable::v2::PingAndWarmRequest;
    use crate::bigtable::proto::bigtable::v2::ReadRowsRequest;
    use crate::bigtable::proto::bigtable::v2::ReadRowsResponse;
    use crate::bigtable::proto::bigtable::v2::RowSet;
    use crate::bigtable::proto::bigtable::v2::bigtable_client::BigtableClient as BigtableInternalClient;

    const TABLE: &str = "t";

    type Rows = Vec<(Bytes, Vec<(Bytes, Bytes)>)>;

    fn metrics(pool: &ChannelPool) -> &Metrics {
        pool.inner.metrics.as_ref().expect("metrics not configured")
    }

    fn suspicions(pool: &ChannelPool, outcome: &str) -> u64 {
        metrics(pool)
            .channel_suspicions
            .with_label_values(&[outcome])
            .get()
    }

    fn entries(pool: &ChannelPool) -> Arc<Vec<Arc<PoolEntry>>> {
        pool.inner.entries.load_full()
    }

    fn in_flight(pool: &ChannelPool) -> Vec<usize> {
        entries(pool)
            .iter()
            .map(|entry| entry.in_flight.load(Ordering::Relaxed))
            .collect()
    }

    struct TestPrimer;

    impl ChannelPrimer for TestPrimer {
        fn prime<'a>(
            &'a self,
            channel: &'a Channel,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                let mut client = BigtableInternalClient::new(channel.clone());
                client
                    .ping_and_warm(PingAndWarmRequest {
                        name: String::new(),
                        app_profile_id: String::new(),
                    })
                    .await?;
                Ok(())
            })
        }
    }

    /// Credentials whose token fetch never succeeds, as while the metadata server is down.
    enum BrokenCredentials {
        /// The fetch fails at once.
        Failing,
        /// The fetch never returns.
        Hanging,
    }

    #[async_trait::async_trait]
    impl TokenProvider for BrokenCredentials {
        async fn token(&self, _scopes: &[&str]) -> Result<Arc<Token>, gcp_auth::Error> {
            if let Self::Hanging = self {
                std::future::pending::<()>().await;
            }
            Err(gcp_auth::Error::Str("metadata server unreachable"))
        }

        async fn project_id(&self) -> Result<Arc<str>, gcp_auth::Error> {
            Err(gcp_auth::Error::Str("metadata server unreachable"))
        }
    }

    /// While `credentials_broken` is set, probes the way the client does with credentials that
    /// cannot be fetched; otherwise probes like [`TestPrimer`].
    struct CredentialsSwitchPrimer {
        credentials_broken: Arc<AtomicBool>,
        broken_credentials: BigtablePrimer,
        working: TestPrimer,
    }

    impl CredentialsSwitchPrimer {
        fn current(&self) -> &dyn ChannelPrimer {
            if self.credentials_broken.load(Ordering::Relaxed) {
                &self.broken_credentials
            } else {
                &self.working
            }
        }
    }

    impl ChannelPrimer for CredentialsSwitchPrimer {
        fn prime<'a>(
            &'a self,
            channel: &'a Channel,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            self.current().prime(channel)
        }

        fn fetch_credentials<'a>(
            &'a self,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            self.current().fetch_credentials()
        }
    }

    async fn start_mock_server() -> (MockBigtableServer, Endpoint, tokio::task::JoinHandle<()>) {
        let mock = MockBigtableServer::new();
        let (addr, handle) = mock.start().await.unwrap();
        let endpoint = Channel::from_shared(format!("http://{addr}")).unwrap();
        (mock, endpoint, handle)
    }

    /// A connected pool of exactly `size` channels primed with [`TestPrimer`], without background
    /// tasks, so tests drive maintenance and health checks themselves.
    async fn fixed_size_pool(
        endpoint: Endpoint,
        size: usize,
        health_check: Option<ChannelHealthConfig>,
        registry: &Registry,
    ) -> ChannelPool {
        let config = PoolConfig {
            initial_pool_size: size,
            min_pool_size: size,
            max_pool_size: size,
            health_check,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, Some(Box::new(TestPrimer)), Some(registry));
        pool.connect().await.unwrap();
        pool
    }

    /// The mock's view of each channel's remote address, in entry order.
    async fn channel_peers(pool: &ChannelPool, mock: &MockBigtableServer) -> Vec<SocketAddr> {
        let mut peers = Vec::new();
        for entry in entries(pool).iter() {
            TestPrimer.prime(&entry.channel).await.unwrap();
            peers.push(*mock.ping_peers().await.last().unwrap());
        }
        peers
    }

    async fn ping(pool: &ChannelPool) {
        BigtableInternalClient::new(pool.clone())
            .ping_and_warm(PingAndWarmRequest {
                name: String::new(),
                app_profile_id: String::new(),
            })
            .await
            .unwrap();
    }

    fn row_key(i: usize) -> Bytes {
        Bytes::from(format!("k{i:04}"))
    }

    /// Rows `0..count` in the mock's table, each with cells `a` and `b` holding the row key.
    async fn insert_rows(mock: &MockBigtableServer, count: usize) {
        for i in 0..count {
            mock.insert_row(TABLE, row_key(i), [("a", row_key(i)), ("b", row_key(i))])
                .await;
        }
    }

    fn expected_rows(keys: &[usize]) -> Rows {
        keys.iter()
            .map(|&i| {
                let cells = vec![
                    (Bytes::from_static(b"a"), row_key(i)),
                    (Bytes::from_static(b"b"), row_key(i)),
                ];
                (row_key(i), cells)
            })
            .collect()
    }

    /// `rows` with each row's cells in qualifier order; the mock emits cells in arbitrary order.
    fn sorted_cells(mut rows: Rows) -> Rows {
        for (_, cells) in &mut rows {
            cells.sort();
        }
        rows
    }

    fn read_rows_request(keys: &[usize]) -> ReadRowsRequest {
        ReadRowsRequest {
            table_name: format!("projects/emulator/instances/test/tables/{TABLE}"),
            rows_limit: keys.len() as i64,
            rows: Some(RowSet {
                row_keys: keys.iter().map(|&i| row_key(i)).collect(),
                row_ranges: vec![],
            }),
            ..ReadRowsRequest::default()
        }
    }

    /// Keys of the rows in a `RawH2Server` response.
    async fn response_keys(mut stream: Streaming<ReadRowsResponse>) -> Vec<Bytes> {
        let mut keys = Vec::new();
        while let Some(message) = stream.message().await.unwrap() {
            keys.extend(message.chunks.into_iter().map(|chunk| chunk.row_key));
        }
        keys
    }

    /// Open `count` `ReadRows` streams on `pool` against a [`RawH2Server`] holding responses
    /// after their headers, in waves of 500.
    async fn open_held_streams(
        pool: &ChannelPool,
        count: usize,
    ) -> Vec<Streaming<ReadRowsResponse>> {
        let client = BigtableInternalClient::new(pool.clone());
        let mut streams = Vec::with_capacity(count);
        let keys: Vec<usize> = (0..count).collect();
        for wave in keys.chunks(500) {
            let opened = join_all(wave.iter().map(|&i| {
                let mut client = client.clone();
                async move {
                    client
                        .read_rows(read_rows_request(&[i]))
                        .await
                        .unwrap()
                        .into_inner()
                }
            }))
            .await;
            streams.extend(opened);
        }
        streams
    }

    #[tokio::test]
    async fn test_pool_creation() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();

        let config = PoolConfig {
            initial_pool_size: 4,
            min_pool_size: 1,
            max_pool_size: 10,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(&registry));
        pool.connect().await.unwrap();

        assert_eq!(metrics(&pool).pool_size.get(), 4);
    }

    #[tokio::test]
    async fn test_pool_creation_with_primer() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();

        let config = PoolConfig {
            initial_pool_size: 3,
            min_pool_size: 1,
            max_pool_size: 10,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(
            endpoint,
            config,
            Some(Box::new(TestPrimer)),
            Some(&registry),
        );
        pool.connect().await.unwrap();

        assert_eq!(metrics(&pool).pool_size.get(), 3);
    }

    #[tokio::test]
    async fn test_round_robin_rpcs() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();

        let config = PoolConfig {
            initial_pool_size: 2,
            min_pool_size: 1,
            max_pool_size: 10,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(
            endpoint,
            config,
            Some(Box::new(TestPrimer)),
            Some(&registry),
        );
        pool.connect().await.unwrap();

        // Send RPCs through the pool via PingAndWarm.
        let n = 6usize;
        for _ in 0..n {
            ping(&pool).await;
        }

        assert_eq!(metrics(&pool).rpcs_completed.get(), n as u64);
        // Mock counts primer pings (2 from connect) + our 6 RPCs.
        assert_eq!(mock.request_count.load(Ordering::Relaxed), 2 + n);

        // Each RPC finishes before the next starts, so both channels are always tied at zero in
        // flight and the rotating scan start splits the RPCs evenly.
        let entries = entries(&pool);
        assert_eq!(entries[0].success_count.load(Ordering::Relaxed), 3);
        assert_eq!(entries[1].success_count.load(Ordering::Relaxed), 3);
    }

    #[tokio::test]
    async fn open_response_streams_count_as_in_flight_and_steer_selection() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        insert_rows(&mock, 2).await;
        let registry = Registry::new();
        let pool = fixed_size_pool(endpoint, 3, None, &registry).await;
        let mut client = BigtableInternalClient::new(pool.clone());

        // Two streams that deliver their first row, then stall with the response still open.
        let mut open_streams = Vec::new();
        for _ in 0..2 {
            mock.interrupt_next_read_rows(ReadRowsInterruption {
                after_rows: 1,
                fault: ReadRowsFault::Stall,
            })
            .await;
            let mut stream = client
                .read_rows(read_rows_request(&[0, 1]))
                .await
                .unwrap()
                .into_inner();
            assert!(stream.message().await.unwrap().is_some());
            open_streams.push(stream);
        }
        // Each stream still counts after its headers and first row arrived.
        let loads = in_flight(&pool);
        assert_eq!(loads.iter().filter(|&&n| n == 1).count(), 2);
        let idle = loads.iter().position(|&n| n == 0).unwrap();

        for _ in 0..4 {
            ping(&pool).await;
        }
        let entries = entries(&pool);
        for (idx, entry) in entries.iter().enumerate() {
            let expected = if idx == idle { 4 } else { 1 };
            assert_eq!(entry.success_count.load(Ordering::Relaxed), expected);
        }

        drop(open_streams);
        assert_eq!(in_flight(&pool), vec![0, 0, 0]);
        assert_eq!(pool.inner.load.in_flight.load(Ordering::Relaxed), 0);
    }

    async fn resize_with_peak(pool: &ChannelPool, peak_in_flight: usize) -> i64 {
        pool.inner
            .load
            .peak_in_flight
            .store(peak_in_flight, Ordering::Relaxed);
        pool.inner.resize().await;
        metrics(pool).pool_size.get()
    }

    #[tokio::test]
    async fn resize_follows_the_interval_peak_and_holds_inside_the_band() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        // Band per peak P: [max(ceil(P / 25), 4), min(P, 10)], at most 2 channels per resize.
        let config = PoolConfig {
            initial_pool_size: 4,
            min_pool_size: 4,
            max_pool_size: 10,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(&registry));
        pool.connect().await.unwrap();

        // A burst that has already ended still sets the interval's peak, and the next interval's
        // peak starts from the requests still in flight.
        let entry = entries(&pool)[0].clone();
        let still_open = InFlightGuard::new(entry.clone());
        let burst: Vec<_> = (0..30).map(|_| InFlightGuard::new(entry.clone())).collect();
        drop(burst);
        pool.inner.resize().await;
        assert_eq!(metrics(&pool).peak_in_flight.get(), 31);
        assert_eq!(metrics(&pool).pool_size.get(), 4);
        assert_eq!(pool.inner.load.peak_in_flight.load(Ordering::Relaxed), 1);
        drop(still_open);

        // Low load never shrinks below the floor.
        assert_eq!(resize_with_peak(&pool, 0).await, 4);

        // Peak 200 needs at least 8 channels: grow 4 -> 6 -> 8, then hold inside [8, 10].
        assert_eq!(resize_with_peak(&pool, 200).await, 6);
        assert_eq!(resize_with_peak(&pool, 200).await, 8);
        assert_eq!(resize_with_peak(&pool, 200).await, 8);

        // Peak 10 allows [4, 10], so 8 channels stay.
        assert_eq!(resize_with_peak(&pool, 10).await, 8);

        // Peak 5 allows [4, 5]: shrink by 2 per resize, dropping quarantined channels first.
        assert_eq!(resize_with_peak(&pool, 5).await, 6);
        let quarantined = entries(&pool)[0].clone();
        pool.inner.quarantine(&quarantined, Instant::now());
        assert_eq!(resize_with_peak(&pool, 5).await, 4);
        assert!(
            entries(&pool)
                .iter()
                .all(|entry| !Arc::ptr_eq(entry, &quarantined))
        );
        assert_eq!(metrics(&pool).quarantined_channels.get(), 0);
        assert_eq!(resize_with_peak(&pool, 5).await, 4);
    }

    #[tokio::test]
    async fn test_age_refresh_replaces_old_channels() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();

        let config = PoolConfig {
            initial_pool_size: 2,
            min_pool_size: 1,
            max_pool_size: 10,
            refresh_age: Duration::from_millis(0),
            refresh_jitter: Duration::ZERO,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(&registry));
        pool.connect().await.unwrap();

        pool.inner.refresh().await;

        // Both entries expired, both replaced (within MAX_REPLACEMENTS_PER_CYCLE=2).
        assert_eq!(metrics(&pool).channels_replaced.get(), 2);
    }

    #[tokio::test]
    async fn test_age_refresh_caps_replacements_per_cycle() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();

        let config = PoolConfig {
            initial_pool_size: 4,
            min_pool_size: 1,
            max_pool_size: 10,
            refresh_age: Duration::from_millis(0),
            refresh_jitter: Duration::ZERO,
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, None, Some(&registry));
        pool.connect().await.unwrap();

        // All 4 entries are expired, but only MAX_REPLACEMENTS_PER_CYCLE=2 replaced per cycle.
        pool.inner.refresh().await;
        assert_eq!(metrics(&pool).channels_replaced.get(), 2);

        // Second cycle replaces the remaining 2.
        pool.inner.refresh().await;
        assert_eq!(metrics(&pool).channels_replaced.get(), 4);
    }

    #[test]
    fn repeat_offenders_must_stand_out_from_the_rest_of_the_pool() {
        // Three quarantines while the other channels had none.
        assert!(is_quarantine_outlier(&[3, 0, 0, 0], 0, 3, 3.0));
        assert!(is_quarantine_outlier(&[3, 1, 0, 0], 0, 3, 3.0));
        // Below the threshold, however quiet the rest of the pool is.
        assert!(!is_quarantine_outlier(&[2, 0, 0, 0], 0, 3, 3.0));
        // A pool-wide bad spell, e.g. legitimately slow reads landing everywhere, implicates no
        // single channel.
        assert!(!is_quarantine_outlier(&[3, 3, 3, 3], 0, 3, 3.0));
        assert!(!is_quarantine_outlier(&[5, 2, 2, 2], 0, 3, 3.0));
    }

    /// An open request on `entry` whose caller is waiting on the channel.
    fn waiting_request(entry: &Arc<PoolEntry>) -> InFlightGuard {
        let mut request = InFlightGuard::new(entry.clone());
        request.start_waiting();
        request
    }

    #[tokio::test(start_paused = true)]
    async fn circuit_breaker_blocks_quarantine_when_most_channels_stall() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig::default();
        let pool = fixed_size_pool(endpoint, 4, Some(health.clone()), &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let entries = entries(&pool);
        for peer in &peers[1..] {
            mock.stall_peer(*peer).await;
        }
        // Two callers waiting on every channel.
        let _waiting: Vec<InFlightGuard> = entries
            .iter()
            .flat_map(|entry| [waiting_request(entry), waiting_request(entry)])
            .collect();

        // Channels 1-3 go silent while channel 0 keeps receiving responses: three stalled
        // channels in a pool of four is more than half, so nothing is probed or quarantined.
        tokio::time::advance(health.suspect_after_silence * 2).await;
        entries[0].record_response();
        let requests_before = mock.request_count.load(Ordering::Relaxed);
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert!(entries.iter().all(|entry| !entry.is_quarantined()));
        assert_eq!(suspicions(&pool, "circuit_breaker_skipped"), 3);
        assert_eq!(mock.request_count.load(Ordering::Relaxed), requests_before);

        // With only channel 1 silent, its probe runs, times out, and quarantines it.
        tokio::time::advance(health.probe_interval).await;
        for idx in [0, 2, 3] {
            entries[idx].record_response();
        }
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        let quarantined: Vec<bool> = entries.iter().map(|e| e.is_quarantined()).collect();
        assert_eq!(quarantined, [false, true, false, false]);
        assert_eq!(suspicions(&pool, "quarantined"), 1);
        assert_eq!(metrics(&pool).quarantined_channels.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn silence_implicates_no_channel_while_no_channel_receives_responses() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig::default();
        let pool = fixed_size_pool(endpoint, 4, Some(health.clone()), &registry).await;
        let entries = entries(&pool);
        let _waiting = [waiting_request(&entries[1]), waiting_request(&entries[1])];

        // Channel 1 has callers waiting and nothing arriving, but channel 0's last response is
        // just as old and no other channel has had one: nothing is probed or quarantined.
        entries[0].record_response();
        tokio::time::advance(health.suspect_after_silence * 2).await;
        let requests_before = mock.request_count.load(Ordering::Relaxed);
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert!(entries.iter().all(|entry| !entry.is_quarantined()));
        assert_eq!(mock.request_count.load(Ordering::Relaxed), requests_before);
        for outcome in ["probe_ok", "quarantined", "circuit_breaker_skipped"] {
            assert_eq!(suspicions(&pool, outcome), 0, "{outcome}");
        }
    }

    #[tokio::test]
    async fn only_callers_waiting_on_a_channel_can_make_it_look_stalled() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig {
            suspect_after_silence: Duration::from_millis(200),
            probe_timeout: Duration::from_secs(5),
            ..ChannelHealthConfig::default()
        };
        let pool = fixed_size_pool(endpoint, 2, Some(health.clone()), &registry).await;
        let entries = entries(&pool);
        let steer_to_channel_0: Vec<_> = (0..10)
            .map(|_| InFlightGuard::new(entries[1].clone()))
            .collect();

        // Two responses on channel 0 send one empty message and then nothing more.
        let mut streams = Vec::new();
        for _ in 0..2 {
            mock.interrupt_next_read_rows(ReadRowsInterruption {
                after_rows: 0,
                fault: ReadRowsFault::Stall,
            })
            .await;
            let stream = BigtableInternalClient::new(pool.clone())
                .read_rows(read_rows_request(&[0]))
                .await
                .unwrap()
                .into_inner();
            streams.push(stream);
        }
        drop(steer_to_channel_0);
        assert_eq!(in_flight(&pool), vec![2, 0]);

        // Their callers are busy elsewhere, as behind a slow downstream client, so however long
        // channel 0 stays silent while channel 1 answers, it is not suspected.
        tokio::time::sleep(health.suspect_after_silence * 2).await;
        ping(&pool).await;
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert_eq!(entries[0].waiting.load(Ordering::Relaxed), 0);
        for outcome in ["probe_ok", "quarantined", "circuit_breaker_skipped"] {
            assert_eq!(suspicions(&pool, outcome), 0, "{outcome}");
        }

        // Once both callers wait on the streams, the silence counts. The connection still
        // answers probes, so the channel stays in service.
        let readers: Vec<_> = streams
            .into_iter()
            .map(|mut stream| {
                tokio::spawn(async move { while let Ok(Some(_)) = stream.message().await {} })
            })
            .collect();
        tokio::time::sleep(health.suspect_after_silence * 2).await;
        assert_eq!(entries[0].waiting.load(Ordering::Relaxed), 2);
        ping(&pool).await;
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert_eq!(suspicions(&pool, "probe_ok"), 1);
        assert!(!entries[0].is_quarantined());
        for reader in readers {
            reader.abort();
        }
    }

    #[tokio::test]
    async fn probe_counts_any_answer_from_the_server_as_alive() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(endpoint, 1, None, &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let entry = entries(&pool)[0].clone();
        let probe_timeout = Duration::from_millis(300);

        // An error status, as from missing the ping permission, still made the round trip.
        mock.ping_should_fail.store(true, Ordering::Relaxed);
        assert_eq!(
            pool.inner.probe(&entry, Duration::from_secs(5)).await,
            ProbeOutcome::Answered
        );

        // No answer does not.
        mock.stall_peer(peers[0]).await;
        assert_eq!(
            pool.inner.probe(&entry, probe_timeout).await,
            ProbeOutcome::NoAnswer
        );
    }

    #[tokio::test]
    async fn probe_cut_off_by_its_connection_closing_is_no_answer() {
        // One stream per connection, so the probe waits for the stream an open read holds.
        let server = RawH2Server::start_with_max_concurrent_streams(1).await;
        let registry = Registry::new();
        let pool = fixed_size_pool(server.endpoint(), 1, None, &registry).await;
        let entry = entries(&pool)[0].clone();
        server.set_read_rows_mode(ReadRowsMode::HoldAfterHeaders);
        let _open_read = open_held_streams(&pool, 1).await;
        let probe = tokio::spawn({
            let pool = pool.clone();
            async move { pool.inner.probe(&entry, Duration::from_secs(30)).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        // The connection closes under the probe, long before its timeout. tonic reports that
        // with a status of its own, which says nothing about the connection working.
        server.close_holding_connections().await;
        let outcome = tokio::time::timeout(Duration::from_secs(5), probe)
            .await
            .expect("the probe waited for its timeout")
            .unwrap();
        assert_eq!(outcome, ProbeOutcome::NoAnswer);
    }

    #[tokio::test]
    async fn probe_whose_credential_fetch_fails_leaves_its_channel_as_it_was() {
        assert_probes_without_credentials_change_nothing(BrokenCredentials::Failing).await;
    }

    #[tokio::test]
    async fn probe_whose_credential_fetch_hangs_leaves_its_channel_as_it_was() {
        assert_probes_without_credentials_change_nothing(BrokenCredentials::Hanging).await;
    }

    async fn assert_probes_without_credentials_change_nothing(credentials: BrokenCredentials) {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig {
            suspect_after_silence: Duration::from_millis(200),
            probe_timeout: Duration::from_secs(1),
            probe_interval: Duration::from_millis(100),
            ..ChannelHealthConfig::default()
        };
        let credentials_broken = Arc::new(AtomicBool::new(false));
        let primer = CredentialsSwitchPrimer {
            credentials_broken: credentials_broken.clone(),
            broken_credentials: BigtablePrimer {
                instance_name: String::new(),
                policy: String::new(),
                token_provider: Some(Arc::new(credentials)),
            },
            working: TestPrimer,
        };
        let config = PoolConfig {
            initial_pool_size: 4,
            min_pool_size: 4,
            max_pool_size: 4,
            health_check: Some(health.clone()),
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new(endpoint, config, Some(Box::new(primer)), Some(&registry));
        pool.connect().await.unwrap();
        let peers = channel_peers(&pool, &mock).await;
        let entries = entries(&pool);

        // A caller waits on channel 0, whose connection is stalled. Channel 1 is quarantined,
        // although its connection works.
        mock.stall_peer(peers[0]).await;
        let steer_to_channel_0: Vec<_> = entries[1..]
            .iter()
            .map(|entry| InFlightGuard::new(entry.clone()))
            .collect();
        let stalled_read = tokio::spawn({
            let pool = pool.clone();
            async move {
                BigtableInternalClient::new(pool)
                    .read_rows(read_rows_request(&[0]))
                    .await
            }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(steer_to_channel_0);
        assert_eq!(in_flight(&pool), vec![1, 0, 0, 0]);
        pool.inner.quarantine(&entries[1], Instant::now());

        // While credentials cannot be fetched, neither probe reaches its connection: the stalled
        // channel stays in service and the quarantined one stays out.
        credentials_broken.store(true, Ordering::Relaxed);
        tokio::time::sleep(health.suspect_after_silence * 2).await;
        ping(&pool).await;
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert!(!entries[0].is_quarantined());
        assert!(entries[1].is_quarantined());
        assert_eq!(suspicions(&pool, "probe_not_sent"), 1);

        // Once credentials work again, the next probes decide.
        credentials_broken.store(false, Ordering::Relaxed);
        tokio::time::sleep(health.probe_interval * 2).await;
        ping(&pool).await;
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert!(entries[0].is_quarantined());
        assert!(!entries[1].is_quarantined());
        assert_eq!(suspicions(&pool, "quarantined"), 1);
        stalled_read.abort();
    }

    #[tokio::test]
    async fn channels_that_fail_with_overload_errors_are_avoided_for_a_while() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(endpoint, 2, None, &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let entries = entries(&pool);

        // Every request on channel 0 fails at once with `Unavailable`, which leaves it with
        // nothing in flight; without a penalty it would get every other request.
        mock.fail_peer(peers[0]).await;
        let mut failures = 0;
        for _ in 0..10 {
            let ping = BigtableInternalClient::new(pool.clone())
                .ping_and_warm(PingAndWarmRequest {
                    name: String::new(),
                    app_profile_id: String::new(),
                })
                .await;
            failures += usize::from(ping.is_err());
        }
        assert_eq!(failures, 1);
        assert_eq!(entries[1].success_count.load(Ordering::Relaxed), 9);

        // A status in the trailers counts too, but only one that points at the channel or what
        // serves it.
        let read_failing_with = |status: Status| {
            let mock = mock.clone();
            let pool = pool.clone();
            async move {
                mock.interrupt_next_read_rows(ReadRowsInterruption {
                    after_rows: 0,
                    fault: ReadRowsFault::Fail(status),
                })
                .await;
                let mut stream = BigtableInternalClient::new(pool)
                    .read_rows(read_rows_request(&[0]))
                    .await
                    .unwrap()
                    .into_inner();
                loop {
                    match stream.message().await {
                        Ok(Some(_)) => {}
                        Ok(None) => return None,
                        Err(status) => return Some(status.code()),
                    }
                }
            }
        };
        let channel_1_penalized = || entries[1].is_penalized(Instant::now());
        let invalid = read_failing_with(Status::invalid_argument("bad request")).await;
        assert_eq!(invalid, Some(Code::InvalidArgument));
        assert!(!channel_1_penalized());
        let unavailable = read_failing_with(Status::unavailable("overloaded")).await;
        assert_eq!(unavailable, Some(Code::Unavailable));
        assert!(channel_1_penalized());
    }

    #[tokio::test(start_paused = true)]
    async fn error_penalty_ends_after_its_duration() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(endpoint, 1, None, &registry).await;
        let entry = entries(&pool)[0].clone();
        entry.penalize();
        tokio::time::advance(ERROR_PENALTY_DURATION - Duration::from_millis(1)).await;
        assert!(entry.is_penalized(Instant::now()));
        tokio::time::advance(Duration::from_millis(1)).await;
        assert!(!entry.is_penalized(Instant::now()));
    }

    #[tokio::test]
    async fn penalized_channel_gets_requests_only_once_the_others_are_a_full_channel_busier() {
        let (_mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(endpoint, 2, None, &registry).await;
        let entries = entries(&pool);
        let full_channel = pool.inner.config.max_rpcs_per_channel;
        let selected = || pool.inner.select_channel(None).unwrap();

        // Channel 1's requests fail fast, so it has nothing in flight; channel 0 is busy but
        // still within the load band.
        entries[1].penalize();
        let mut busy: Vec<_> = (1..full_channel)
            .map(|_| InFlightGuard::new(entries[0].clone()))
            .collect();
        for _ in 0..2 {
            assert!(Arc::ptr_eq(&selected(), &entries[0]));
        }
        busy.extend((0..2).map(|_| InFlightGuard::new(entries[0].clone())));
        assert!(Arc::ptr_eq(&selected(), &entries[1]));
    }

    #[tokio::test]
    async fn http_error_without_a_grpc_status_penalizes_the_channel() {
        // A proxy in front of BigTable can answer with a bare HTTP error, which the caller sees
        // as `Unavailable`.
        let server = RawH2Server::start().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(server.endpoint(), 1, None, &registry).await;
        server.set_read_rows_mode(ReadRowsMode::ServiceUnavailable);
        let mut client = BigtableInternalClient::new(pool.clone());
        let status = match client.read_rows(read_rows_request(&[0])).await {
            Ok(response) => response.into_inner().message().await.unwrap_err(),
            Err(status) => status,
        };
        assert_eq!(status.code(), Code::Unavailable);
        assert!(entries(&pool)[0].is_penalized(Instant::now()));
    }

    #[tokio::test(start_paused = true)]
    async fn repeat_offender_is_replaced_once_it_stands_out() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig::default();
        let pool = fixed_size_pool(endpoint, 4, Some(health.clone()), &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let entries = entries(&pool);
        mock.stall_peer(peers[0]).await;
        // Channel 0 was quarantined twice recently; no other channel was.
        for _ in 0..2 {
            entries[0]
                .health
                .lock()
                .unwrap()
                .recent_quarantines
                .push_back(Instant::now());
        }
        let _waiting = [waiting_request(&entries[0]), waiting_request(&entries[0])];

        // Its third quarantine makes it an outlier, so the check asks for its replacement.
        tokio::time::advance(health.suspect_after_silence * 2).await;
        entries[1].record_response();
        let (offender, reason) = pool
            .inner
            .check_channel_health(&health)
            .await
            .expect("channel 0 is a repeat offender");
        assert!(Arc::ptr_eq(&offender, &entries[0]));
        assert_eq!(reason, ReplacementReason::RepeatOffender);
    }

    #[tokio::test(start_paused = true)]
    async fn replacement_is_make_before_break_and_rate_limited() {
        let (mock, endpoint, handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig::default();
        let pool = fixed_size_pool(endpoint, 4, Some(health.clone()), &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let original = entries(&pool);
        for idx in [0, 1] {
            mock.stall_peer(peers[idx]).await;
            pool.inner.quarantine(&original[idx], Instant::now());
        }

        // Both channels are still failing re-probes when their replacement deadline passes.
        tokio::time::advance(health.replace_after_quarantined_for).await;
        let (stuck, reason) = pool
            .inner
            .check_channel_health(&health)
            .await
            .expect("channel 0 is due for replacement");
        assert_eq!(reason, ReplacementReason::StillFailingProbes);
        assert!(Arc::ptr_eq(&stuck, &original[0]));
        // The replacement's probe must succeed over a real socket, which a paused clock could
        // time out early by jumping to the probe's deadline while the answer is in transit.
        tokio::time::resume();
        assert!(
            pool.inner
                .replace_unhealthy_channel(&stuck, reason, health.probe_timeout)
                .await
        );
        tokio::time::pause();
        let current = entries(&pool);
        assert_eq!(current.len(), 4);
        assert!(!Arc::ptr_eq(&current[0], &original[0]));
        assert!(!current[0].is_quarantined());
        assert!(Arc::ptr_eq(&current[1], &original[1]));
        let replaced = metrics(&pool)
            .unhealthy_channels_replaced
            .with_label_values(&["still_failing_probes"]);
        assert_eq!(replaced.get(), 1);
        assert_eq!(metrics(&pool).quarantined_channels.get(), 1);

        // Channel 1 is just as stuck, but the pool replaces at most one channel per interval.
        tokio::time::advance(health.probe_interval).await;
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        tokio::time::advance(health.min_replacement_interval).await;
        let (stuck, reason) = pool
            .inner
            .check_channel_health(&health)
            .await
            .expect("channel 1 is due once the interval has passed");
        assert!(Arc::ptr_eq(&stuck, &original[1]));

        // A replacement that connects but never answers its probe leaves the old channel in
        // place. The probe runs in real time, so only the stalled ping can fail it.
        mock.ping_should_stall.store(true, Ordering::Relaxed);
        tokio::time::resume();
        assert!(
            !pool
                .inner
                .replace_unhealthy_channel(&stuck, reason, health.probe_timeout)
                .await
        );
        tokio::time::pause();
        assert!(Arc::ptr_eq(&entries(&pool)[1], &original[1]));
        mock.ping_should_stall.store(false, Ordering::Relaxed);

        // So does a replacement that cannot connect.
        handle.abort();
        let _ = handle.await;
        assert!(
            !pool
                .inner
                .replace_unhealthy_channel(&stuck, reason, health.probe_timeout)
                .await
        );
        let current = entries(&pool);
        assert_eq!(current.len(), 4);
        assert!(Arc::ptr_eq(&current[1], &original[1]));
        assert_eq!(replaced.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn channel_that_left_the_pool_during_its_probe_does_not_take_the_replacement_slot() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig::default();
        let pool = fixed_size_pool(endpoint, 4, Some(health.clone()), &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let original = entries(&pool);
        for idx in [0, 1] {
            mock.stall_peer(peers[idx]).await;
            pool.inner.quarantine(&original[idx], Instant::now());
        }
        tokio::time::advance(health.replace_after_quarantined_for).await;

        // Both channels are due for replacement, but a resize drops channel 0 while its re-probe
        // waits on the stalled peer. The replacement slot goes to channel 1, still in the pool.
        let check = tokio::spawn({
            let pool = pool.clone();
            let health = health.clone();
            async move { pool.inner.check_channel_health(&health).await }
        });
        tokio::time::sleep(health.probe_timeout / 2).await;
        pool.inner.set_entries(original[1..].to_vec());
        let (stuck, reason) = check
            .await
            .unwrap()
            .expect("channel 1 is due for replacement");
        assert!(Arc::ptr_eq(&stuck, &original[1]));
        assert_eq!(reason, ReplacementReason::StillFailingProbes);
    }

    #[tokio::test]
    async fn suspect_that_left_the_pool_during_its_probe_still_has_its_requests_failed() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig {
            suspect_after_silence: Duration::from_millis(200),
            probe_timeout: Duration::from_millis(500),
            ..ChannelHealthConfig::default()
        };
        let pool = fixed_size_pool(endpoint, 4, Some(health.clone()), &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let original = entries(&pool);

        // A caller waits on channel 0, whose connection is stalled.
        mock.stall_peer(peers[0]).await;
        let steer_to_channel_0: Vec<_> = original[1..]
            .iter()
            .map(|entry| InFlightGuard::new(entry.clone()))
            .collect();
        let stalled_read = tokio::spawn({
            let pool = pool.clone();
            async move {
                BigtableInternalClient::new(pool)
                    .read_rows(read_rows_request(&[0]))
                    .await
            }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(steer_to_channel_0);
        assert_eq!(in_flight(&pool), vec![1, 0, 0, 0]);
        tokio::time::sleep(health.suspect_after_silence * 2).await;
        ping(&pool).await;

        // A resize drops channel 0 while its probe waits on the stalled connection. The failed
        // probe still fails the read left on the channel.
        let check = tokio::spawn({
            let pool = pool.clone();
            let health = health.clone();
            async move { pool.inner.check_channel_health(&health).await }
        });
        tokio::time::sleep(health.probe_timeout / 2).await;
        pool.inner.set_entries(original[1..].to_vec());
        assert!(check.await.unwrap().is_none());
        assert!(original[0].is_quarantined());
        // Only the branch for channels outside the pool quarantines without counting a suspicion.
        assert_eq!(suspicions(&pool, "quarantined"), 0);
        let status = tokio::time::timeout(Duration::from_secs(5), stalled_read)
            .await
            .expect("the read on the dropped channel is still stuck")
            .unwrap()
            .unwrap_err();
        assert!(is_quarantine_cancellation(&status));
    }

    #[tokio::test]
    async fn header_stall_trips_the_gap_timeout_and_the_retry_avoids_the_channel() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        insert_rows(&mock, 3).await;
        let registry = Registry::new();
        let pool = fixed_size_pool(endpoint, 2, None, &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        mock.stall_peer(peers[0]).await;
        let mut client = BigTableClient::for_local_pool(pool.clone(), "test", "test", false)
            .with_read_rows_retry(ReadRowsRetryConfig {
                gap_timeout: Duration::from_secs(1),
                max_attempts: 3,
            });
        let kv_metrics = KvMetrics::new(&registry);
        client.metrics = Some(kv_metrics.clone());

        // Channel 1 has more requests open than the error penalty adds, so the first attempt
        // goes to channel 0, which never sends response headers. Once that attempt is abandoned
        // channel 0 is again the least loaded, penalty included, so only avoiding the channel the
        // attempt failed on sends the retry to channel 1; retrying on channel 0 would hang on the
        // final attempt.
        let entries = entries(&pool);
        let busy: Vec<_> = (0..=pool.inner.config.max_rpcs_per_channel)
            .map(|_| InFlightGuard::new(entries[1].clone()))
            .collect();
        let rows = tokio::time::timeout(
            Duration::from_secs(10),
            client.read_rows(read_rows_request(&[0, 1, 2]), TABLE),
        )
        .await
        .expect("the retry went back to the stalled channel")
        .unwrap();
        drop(busy);
        assert_eq!(sorted_cells(rows), expected_rows(&[0, 1, 2]));
        let labels = |label| ["test", TABLE, label];
        let gap_timeouts = &kv_metrics.kv_bt_read_rows_gap_timeouts_total;
        assert_eq!(gap_timeouts.with_label_values(&labels("headers")).get(), 1);
        let retries = &kv_metrics.kv_bt_read_rows_retries_total;
        assert_eq!(retries.with_label_values(&labels("gap_timeout")).get(), 1);
        assert_eq!(entries[0].success_count.load(Ordering::Relaxed), 0);
        assert_eq!(entries[1].success_count.load(Ordering::Relaxed), 1);
        // The abandoned attempt released channel 0, and its stall makes new requests prefer
        // other channels for a while.
        assert_eq!(in_flight(&pool), vec![0, 0]);
        assert!(
            entries
                .iter()
                .all(|entry| entry.waiting.load(Ordering::Relaxed) == 0)
        );
        assert!(entries[0].is_penalized(Instant::now()));
    }

    #[tokio::test]
    async fn reads_on_a_connection_that_closes_resume_elsewhere() {
        // One stream per connection, so requests beyond the first queue inside the client.
        let server = RawH2Server::start_with_max_concurrent_streams(1).await;
        let registry = Registry::new();
        let pool = fixed_size_pool(server.endpoint(), 2, None, &registry).await;
        let entries = entries(&pool);
        let mut client = BigTableClient::for_local_pool(pool.clone(), "test", "test", false);
        let kv_metrics = KvMetrics::new(&registry);
        client.metrics = Some(kv_metrics.clone());
        let steer_to_channel_0: Vec<_> = (0..10)
            .map(|_| InFlightGuard::new(entries[1].clone()))
            .collect();
        let read = |i: usize| {
            let mut client = client.clone();
            tokio::spawn(async move { client.read_rows(read_rows_request(&[i]), TABLE).await })
        };

        // An open read takes channel 0's only stream, and the next request waits inside h2 for
        // it. Requests after that wait in hyper's queue, which hyper drops unsent when the
        // connection closes.
        server.set_read_rows_mode(ReadRowsMode::HoldAfterHeaders);
        let open_read = read(0);
        server.wait_for_unanswered_requests(1).await;
        let waiting_for_a_stream = tokio::spawn({
            let mut raw_client = BigtableInternalClient::new(pool.clone());
            async move { raw_client.read_rows(read_rows_request(&[1])).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let queued_reads = [read(2), read(3)];
        tokio::time::sleep(Duration::from_millis(100)).await;

        // The connection ends without a GOAWAY: the open read fails with an h2 I/O error, the
        // queued ones as never sent. All three resume on channel 1.
        drop(steer_to_channel_0);
        server.set_read_rows_mode(ReadRowsMode::Respond);
        server.close_holding_connections().await;
        let reads = std::iter::once(open_read).chain(queued_reads);
        for (i, read) in [0, 2, 3].into_iter().zip_debug_eq(reads) {
            let rows = tokio::time::timeout(Duration::from_secs(5), read)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let keys: Vec<Bytes> = rows.into_iter().map(|(key, _)| key).collect();
            assert_eq!(keys, vec![row_key(i)]);
        }
        let retries = kv_metrics
            .kv_bt_read_rows_retries_total
            .with_label_values(&["test", TABLE, "other"]);
        assert_eq!(retries.get(), 3);
        assert_eq!(entries[1].success_count.load(Ordering::Relaxed), 3);
        assert!(entries[0].is_penalized(Instant::now()));
        waiting_for_a_stream.abort();
    }

    #[tokio::test]
    async fn one_stalled_read_gets_its_channel_quarantined_and_retries_elsewhere() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        insert_rows(&mock, 16).await;
        let registry = Registry::new();
        let health = ChannelHealthConfig {
            check_interval: Duration::from_millis(50),
            suspect_after_silence: Duration::from_millis(200),
            probe_timeout: Duration::from_millis(100),
            ..ChannelHealthConfig::default()
        };
        let config = PoolConfig {
            initial_pool_size: 4,
            min_pool_size: 4,
            max_pool_size: 4,
            health_check: Some(health),
            ..PoolConfig::default()
        };
        let pool = ChannelPool::new_connected(
            endpoint,
            config,
            Some(Box::new(TestPrimer)),
            Some(&registry),
        )
        .await
        .unwrap();
        let peers = channel_peers(&pool, &mock).await;
        mock.stall_peer(peers[0]).await;
        // A gap timeout that never fires here, so only quarantine can end a stalled attempt.
        let mut client = BigTableClient::for_local_pool(pool.clone(), "test", "test", false)
            .with_read_rows_retry(ReadRowsRetryConfig {
                gap_timeout: Duration::from_secs(3600),
                max_attempts: 3,
            });
        let kv_metrics = KvMetrics::new(&registry);
        client.metrics = Some(kv_metrics.clone());

        // Four concurrent reads start together, so selection puts one on each channel; the one
        // on channel 0 never gets response headers, and no other caller waits on that channel.
        let (finished_tx, mut finished) = mpsc::channel(4);
        let reads: Vec<_> = (0..4usize)
            .map(|read| {
                let mut client = client.clone();
                let finished_tx = finished_tx.clone();
                async move {
                    let keys = [read * 2, read * 2 + 1];
                    let rows = client.read_rows(read_rows_request(&keys), TABLE).await;
                    finished_tx
                        .send((keys, rows.map(sorted_cells), Instant::now()))
                        .await
                        .unwrap();
                }
            })
            .collect();
        tokio::spawn(join_all(reads));
        for _ in 0..3 {
            let (keys, rows, _) = tokio::time::timeout(Duration::from_secs(10), finished.recv())
                .await
                .expect("a read on a healthy channel is stuck")
                .unwrap();
            assert_eq!(rows.unwrap(), expected_rows(&keys));
        }

        // Steady traffic on the other channels, which channel 0's silence is judged against. The
        // background health checker then quarantines channel 0, cancelling the stalled read,
        // which resumes elsewhere and returns exactly its rows. Nothing but the quarantine can
        // end it sooner than the hour-long gap timeout; the bound leaves room for the retry
        // round trip on a busy test host.
        let traffic = tokio::spawn({
            let pool = pool.clone();
            async move {
                loop {
                    ping(&pool).await;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        });
        let (keys, rows, finished_at) =
            tokio::time::timeout(Duration::from_secs(10), finished.recv())
                .await
                .expect("the read on the stalled channel is still stuck")
                .unwrap();
        assert_eq!(rows.unwrap(), expected_rows(&keys));
        traffic.abort();
        let entries = entries(&pool);
        assert!(entries[0].is_quarantined());
        let quarantined_at = entries[0].health.lock().unwrap().quarantined_at.unwrap();
        let elapsed = finished_at.saturating_duration_since(quarantined_at);
        assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
        assert_eq!(entries[0].in_flight.load(Ordering::Relaxed), 0);
        assert_eq!(entries[0].waiting.load(Ordering::Relaxed), 0);
        assert_eq!(suspicions(&pool, "quarantined"), 1);
        assert_eq!(metrics(&pool).channel_quarantines.get(), 1);
        assert_eq!(metrics(&pool).requests_cancelled_by_quarantine.get(), 1);
        assert_eq!(metrics(&pool).quarantined_channels.get(), 1);
        let retries = kv_metrics
            .kv_bt_read_rows_retries_total
            .with_label_values(&["test", TABLE, "quarantine"]);
        assert_eq!(retries.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn quarantined_channel_gets_no_requests_until_a_probe_restores_it() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let health = ChannelHealthConfig::default();
        let pool = fixed_size_pool(endpoint, 4, Some(health.clone()), &registry).await;
        let peers = channel_peers(&pool, &mock).await;
        let entries = entries(&pool);
        mock.stall_peer(peers[0]).await;
        pool.inner.quarantine(&entries[0], Instant::now());

        // While its re-probes fail, the channel stays out of selection.
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert!(entries[0].is_quarantined());
        for _ in 0..8 {
            ping(&pool).await;
        }
        assert_eq!(entries[0].success_count.load(Ordering::Relaxed), 0);

        // Once the connection answers again, the next re-probe returns it to service. That
        // probe must succeed over a real socket, which a paused clock could time out early.
        mock.unstall_peer(peers[0]).await;
        tokio::time::advance(health.probe_interval).await;
        tokio::time::resume();
        assert!(pool.inner.check_channel_health(&health).await.is_none());
        assert!(!entries[0].is_quarantined());
        assert_eq!(metrics(&pool).channel_restores.get(), 1);
        assert_eq!(metrics(&pool).quarantined_channels.get(), 0);
        for _ in 0..8 {
            ping(&pool).await;
        }
        assert_eq!(entries[0].success_count.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn quarantine_cancellation_does_not_slow_batch_writes() {
        let (mock, endpoint, _handle) = start_mock_server().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(endpoint, 2, None, &registry).await;
        let mut client = BigTableClient::for_local_pool(pool.clone(), "test", "flow-control", true)
            .with_backfill_through(10);
        let effective_qps = |client: &BigTableClient| {
            client
                .flow_controller
                .as_ref()
                .expect("batch-write flow control is enabled")
                .effective_qps()
        };
        let initial_qps = effective_qps(&client);
        let make_entry = || {
            crate::tables::make_entry(
                Bytes::from_static(b"flow-control-row"),
                [("col", Bytes::from_static(b"value"))],
                None,
            )
        };

        // Quarantine the channel carrying a write while BigTable holds it.
        let gate = mock.pause_next_mutate_rows().await;
        let write = tokio::spawn({
            let mut client = client.clone();
            let entry = make_entry();
            async move {
                client
                    .write_entries("flow-control", [entry], CheckpointSpan::single(10))
                    .await
            }
        });
        gate.wait_for_arrival().await;
        let busy = entries(&pool)
            .iter()
            .find(|entry| entry.in_flight.load(Ordering::Relaxed) == 1)
            .cloned()
            .expect("the write is in flight on one channel");
        pool.inner.quarantine(&busy, Instant::now());
        let error = tokio::time::timeout(Duration::from_secs(10), write)
            .await
            .expect("quarantine did not end the held write")
            .unwrap()
            .unwrap_err();
        assert!(is_quarantine_cancellation(
            error.downcast_ref::<Status>().unwrap()
        ));
        gate.release();

        // An overload error would lower the rate within these writes; the local cancellation
        // says nothing about BigTable's load, so the rate never drops.
        for _ in 0..20 {
            client
                .write_entries("flow-control", [make_entry()], CheckpointSpan::single(10))
                .await
                .unwrap();
            assert!(effective_qps(&client) >= initial_qps);
        }
    }

    #[tokio::test]
    async fn mass_cancel_on_one_connection_causes_no_goaway() {
        let server = RawH2Server::start().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(server.endpoint(), 1, None, &registry).await;
        let entry = entries(&pool)[0].clone();

        server.set_read_rows_mode(ReadRowsMode::HoldAfterHeaders);
        let streams = open_held_streams(&pool, 1_500).await;
        assert_eq!(entry.in_flight.load(Ordering::Relaxed), 1_500);
        let readers: Vec<_> = streams
            .into_iter()
            .map(|mut stream| tokio::spawn(async move { stream.message().await }))
            .collect();

        // Let every reader start waiting on its stream, so the quarantine has to wake it.
        tokio::time::timeout(Duration::from_secs(5), async {
            while entry.waiting.load(Ordering::Relaxed) < 1_500 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("readers did not start waiting");

        // Quarantine fails every open stream locally, and each failure resets its HTTP/2 stream.
        pool.inner.quarantine(&entry, Instant::now());
        for reader in readers {
            let status = tokio::time::timeout(Duration::from_secs(5), reader)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert!(is_quarantine_cancellation(&status), "{status:?}");
        }
        assert_eq!(entry.in_flight.load(Ordering::Relaxed), 0);
        assert_eq!(entry.waiting.load(Ordering::Relaxed), 0);
        server.wait_for_resets(1_500).await;

        // The server keeps sending DATA on every stream the client reset. Most of those streams
        // are past h2's short memory of locally reset streams; if h2 answered each late frame
        // with its own reset, the 1024 local-reset limit would make it GOAWAY.
        server.flood_reset_streams(512).await;
        assert_eq!(server.client_goaway(), None);

        // The same connection still serves a probe and new reads.
        assert_eq!(
            pool.inner.probe(&entry, Duration::from_secs(5)).await,
            ProbeOutcome::Answered
        );
        pool.inner.restore(&entry);
        server.set_read_rows_mode(ReadRowsMode::Respond);
        let stream = BigtableInternalClient::new(pool.clone())
            .read_rows(read_rows_request(&[7, 8]))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response_keys(stream).await, vec![row_key(7), row_key(8)]);
        assert_eq!(server.client_goaway(), None);
        assert_eq!(server.connections(), 1);
    }

    #[tokio::test]
    async fn late_headers_on_cancelled_streams_cause_no_goaway() {
        let server = RawH2Server::start().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(server.endpoint(), 1, None, &registry).await;
        let entry = entries(&pool)[0].clone();

        // 1,500 reads wait for response headers that do not come, as in a stalled connection.
        server.set_read_rows_mode(ReadRowsMode::Ignore);
        let client = BigtableInternalClient::new(pool.clone());
        let reads: Vec<_> = (0..1_500)
            .map(|i| {
                let mut client = client.clone();
                tokio::spawn(async move { client.read_rows(read_rows_request(&[i])).await })
            })
            .collect();
        server.wait_for_unanswered_requests(1_500).await;

        pool.inner.quarantine(&entry, Instant::now());
        for read in reads {
            let status = tokio::time::timeout(Duration::from_secs(5), read)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err();
            assert!(is_quarantine_cancellation(&status), "{status:?}");
        }
        server.wait_for_resets(1_500).await;

        // The server answers every cancelled request after all: HEADERS, then DATA.
        server.answer_ignored_streams(512).await;
        assert_eq!(server.client_goaway(), None);

        assert_eq!(
            pool.inner.probe(&entry, Duration::from_secs(5)).await,
            ProbeOutcome::Answered
        );
        pool.inner.restore(&entry);
        server.set_read_rows_mode(ReadRowsMode::Respond);
        let stream = BigtableInternalClient::new(pool.clone())
            .read_rows(read_rows_request(&[7, 8]))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response_keys(stream).await, vec![row_key(7), row_key(8)]);
        assert_eq!(server.client_goaway(), None);
        assert_eq!(server.connections(), 1);
    }

    #[tokio::test]
    async fn dropping_most_streams_leaves_the_rest_intact() {
        let server = RawH2Server::start().await;
        let registry = Registry::new();
        let pool = fixed_size_pool(server.endpoint(), 1, None, &registry).await;

        server.set_read_rows_mode(ReadRowsMode::HoldAfterHeaders);
        let streams = open_held_streams(&pool, 1_500).await;
        // Keep every fifth stream and drop the other 1,200, resetting them.
        let survivors: Vec<(usize, Streaming<ReadRowsResponse>)> = streams
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 5 == 0)
            .collect();
        server.wait_for_resets(1_200).await;

        server.flood_reset_streams(512).await;
        server.complete_held_streams().await;
        for (i, stream) in survivors {
            assert_eq!(response_keys(stream).await, vec![row_key(i)]);
        }
        assert_eq!(server.client_goaway(), None);
        assert_eq!(server.connections(), 1);
    }
}
