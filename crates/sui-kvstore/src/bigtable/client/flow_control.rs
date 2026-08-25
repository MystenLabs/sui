// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Bigtable batch-write flow control starts configured clients at a conservative `MutateRows`
//! admission rate, then derives later limits from observed RPC starts. Server decreases,
//! qualifying RPC errors, and write latency contribute feedback to complete observation windows.
//! Rate increases require healthy latency relative to the learned baseline and sufficient demand.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;
use tonic::Code;
use tracing::info;

use crate::bigtable::metrics::KvMetrics;
use crate::bigtable::proto::bigtable::v2::RateLimitInfo;

const DEFAULT_PERIOD: Duration = Duration::from_secs(10);
const OBSERVATION_WINDOW: Duration = Duration::from_secs(1);
const LATENCY_EVALUATION_PERIOD: Duration = Duration::from_secs(10);
const INITIAL_QPS: f64 = 10.0;
const MIN_QPS: f64 = 1.0;
const MAX_QPS: f64 = 100_000.0;
const MIN_FACTOR: f64 = 0.7;
const MAX_FACTOR: f64 = 1.3;
const HEALTHY_RECOVERY_FACTOR: f64 = 1.05;
const UPWARD_UTILIZATION_THRESHOLD: f64 = 0.8;
const ELEVATED_LATENCY_RATIO: f64 = 1.5;
const SEVERE_LATENCY_RATIO: f64 = 3.0;
const BASELINE_EWMA_ALPHA: f64 = 0.2;
// Sustained non-healthy latency moves the baseline halfway toward the observed latency every ten
// minutes of bounded sample evidence.
const NON_HEALTHY_BASELINE_HALF_LIFE: Duration = Duration::from_secs(10 * 60);
const MIN_WINDOW_SAMPLES: u64 = 5;

pub(super) fn is_overload_error(code: Code) -> bool {
    matches!(
        code,
        Code::DeadlineExceeded | Code::Unavailable | Code::ResourceExhausted
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WriteLatencyCondition {
    Healthy,
    Elevated,
    Severe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LatencyFeedback {
    Decrease,
    Reanchor,
    Increase,
}

impl LatencyFeedback {
    fn factor(self) -> f64 {
        match self {
            Self::Decrease => MIN_FACTOR,
            Self::Reanchor => 1.0,
            Self::Increase => HEALTHY_RECOVERY_FACTOR,
        }
    }

    fn direction_label(self) -> &'static str {
        match self {
            Self::Decrease => "decrease",
            Self::Reanchor => "reanchor",
            Self::Increase => "increase",
        }
    }

    fn reanchors_to_observed(self) -> bool {
        matches!(self, Self::Decrease | Self::Reanchor)
    }

    fn allows_growth(self) -> bool {
        self == Self::Increase
    }

    fn more_conservative(self, other: Self) -> Self {
        match (self, other) {
            (Self::Decrease, _) | (_, Self::Decrease) => Self::Decrease,
            (Self::Reanchor, _) | (_, Self::Reanchor) => Self::Reanchor,
            (Self::Increase, Self::Increase) => Self::Increase,
        }
    }
}

fn classify_write_latency(
    window_avg_micros: f64,
    baseline_micros: Option<f64>,
) -> WriteLatencyCondition {
    if baseline_micros.is_some_and(|baseline| window_avg_micros > SEVERE_LATENCY_RATIO * baseline) {
        WriteLatencyCondition::Severe
    } else if baseline_micros
        .is_some_and(|baseline| window_avg_micros > ELEVATED_LATENCY_RATIO * baseline)
    {
        WriteLatencyCondition::Elevated
    } else {
        WriteLatencyCondition::Healthy
    }
}

fn non_healthy_baseline_alpha(evidence_duration: Duration) -> f64 {
    1.0 - 2.0_f64
        .powf(-evidence_duration.as_secs_f64() / NON_HEALTHY_BASELINE_HALF_LIFE.as_secs_f64())
}

struct ServerFeedback {
    factor: f64,
    period: Duration,
}

impl ServerFeedback {
    fn direction_label(&self) -> &'static str {
        if self.factor < 1.0 {
            "decrease"
        } else if self.factor > 1.0 {
            "increase"
        } else {
            "neutral"
        }
    }
}

struct ObservationWindow {
    started_at: Instant,
    rpc_starts: u64,
}

#[derive(Default)]
struct PendingFeedback {
    server_factor: Option<f64>,
    latency_feedback: Option<LatencyFeedback>,
    server_period: Option<Duration>,
}

impl PendingFeedback {
    fn combined_factor(&self) -> Option<f64> {
        let latency_factor = self.latency_feedback.map(LatencyFeedback::factor);
        match (self.server_factor, latency_factor) {
            (Some(server_factor), Some(latency_factor)) => Some(server_factor.min(latency_factor)),
            (Some(server_factor), None) => Some(server_factor),
            (None, Some(latency_factor)) => Some(latency_factor),
            (None, None) => None,
        }
    }

    fn allows_growth(&self) -> bool {
        self.latency_feedback
            .is_some_and(LatencyFeedback::allows_growth)
    }

    fn growth_factor(&self) -> Option<f64> {
        let latency_feedback = self
            .latency_feedback
            .filter(|feedback| feedback.allows_growth())?;
        match self.server_factor {
            Some(server_factor) if server_factor <= 1.0 => None,
            Some(server_factor) => Some(server_factor),
            None => Some(latency_feedback.factor()),
        }
    }

    fn reanchors_to_observed(&self) -> bool {
        self.server_factor.is_some_and(|factor| factor < 1.0)
            || self
                .latency_feedback
                .is_some_and(LatencyFeedback::reanchors_to_observed)
    }

    fn discard_growth_feedback(&mut self) {
        if !self.allows_growth() {
            return;
        }
        self.server_factor = self.server_factor.filter(|factor| *factor <= 1.0);
        self.latency_feedback = self
            .latency_feedback
            .filter(|feedback| !feedback.allows_growth());
        self.server_period = self.server_factor.and(self.server_period);
    }

    fn is_empty(&self) -> bool {
        self.server_factor.is_none() && self.latency_feedback.is_none()
    }
}

struct ControllerState {
    effective_qps: f64,
    // Rate changes invalidate sleeping reservations and latency samples from the prior rate.
    rate_generation: u64,
    next_permit_at: Instant,
    observation: Option<ObservationWindow>,
    pending: PendingFeedback,
    next_server_update_at: Instant,
    next_latency_evaluation_at: Instant,
    latency_total_micros: u64,
    latency_samples: u64,
    latency_sampling_started_at: Option<Instant>,
    baseline_write_latency_micros: Option<f64>,
}

impl ControllerState {
    fn permit_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.effective_qps)
    }

    fn growth_observation_timeout(&self) -> Duration {
        self.permit_interval().saturating_add(OBSERVATION_WINDOW)
    }

    // Limit adaptation credit to the expected sampling cadence so idle time cannot redefine the
    // baseline.
    fn baseline_evidence_cap(&self) -> Duration {
        let minimum_sample_duration =
            Duration::from_secs_f64(MIN_WINDOW_SAMPLES as f64 / self.effective_qps);
        LATENCY_EVALUATION_PERIOD.max(minimum_sample_duration)
    }

    fn restart_or_close_empty_observation(&mut self, now: Instant) {
        self.pending.discard_growth_feedback();
        if self.pending.is_empty() {
            self.observation = None;
            return;
        }
        self.observation
            .as_mut()
            .expect("observed feedback window disappeared")
            .started_at = now;
    }
    fn restart_or_close_empty_observation_if_ready(&mut self, now: Instant) {
        let Some(observation) = self.observation.as_ref() else {
            return;
        };
        let elapsed = now.saturating_duration_since(observation.started_at);
        if elapsed < OBSERVATION_WINDOW || observation.rpc_starts != 0 {
            return;
        }

        // A low paced rate can make the fixed window empty even under sustained demand.
        // Growth-only feedback remains valid through the next permit interval and one
        // observation boundary. Conservative feedback restarts without including idle time.
        if self.pending.growth_factor().is_none() || elapsed >= self.growth_observation_timeout() {
            self.restart_or_close_empty_observation(now);
        }
    }

    fn reserve_permit(&mut self, now: Instant) -> PermitReservation {
        let permit_interval = self.permit_interval();
        let permit_at = self.next_permit_at.max(now);
        self.next_permit_at = permit_at + permit_interval;
        PermitReservation {
            wait: permit_at.saturating_duration_since(now),
            rate_generation: self.rate_generation,
        }
    }

    fn complete_reservation(&mut self, reservation: PermitReservation) -> bool {
        if reservation.rate_generation != self.rate_generation {
            return false;
        }
        if let Some(observation) = self.observation.as_mut() {
            observation.rpc_starts = observation.rpc_starts.saturating_add(1);
        }
        true
    }
}

struct CompletedObservation {
    observed_start_qps: f64,
    effective_qps: f64,
}

#[must_use = "write admissions must be completed with the RPC outcome"]
pub(crate) struct WriteAdmission<'a> {
    flow_controller: &'a BatchWriteFlowController,
    started_at: Instant,
    rate_generation: u64,
}

#[derive(Clone, Copy)]
struct PermitReservation {
    wait: Duration,
    rate_generation: u64,
}

struct LatencyEvaluation {
    window_avg_micros: f64,
    baseline_micros: Option<f64>,
    condition: WriteLatencyCondition,
    feedback: Option<LatencyFeedback>,
    started_observation: bool,
}

pub(crate) struct BatchWriteFlowController {
    state: Mutex<ControllerState>,
    client_name: String,
    metrics: Option<Arc<KvMetrics>>,
}

impl BatchWriteFlowController {
    pub(crate) fn new(client_name: String, metrics: Option<Arc<KvMetrics>>) -> Arc<Self> {
        let now = Instant::now();
        let controller = Arc::new(Self {
            state: Mutex::new(ControllerState {
                effective_qps: INITIAL_QPS,
                rate_generation: 0,
                next_permit_at: now,
                observation: None,
                pending: PendingFeedback::default(),
                next_server_update_at: now,
                next_latency_evaluation_at: now + LATENCY_EVALUATION_PERIOD,
                latency_total_micros: 0,
                latency_samples: 0,
                latency_sampling_started_at: None,
                baseline_write_latency_micros: None,
            }),
            client_name,
            metrics,
        });

        if let Some(metrics) = &controller.metrics {
            metrics
                .kv_bt_flow_control_effective_qps
                .with_label_values(&[&controller.client_name])
                .set(INITIAL_QPS);
            metrics
                .kv_bt_flow_control_last_observation_start_qps
                .with_label_values(&[&controller.client_name])
                .set(0.0);
        }
        info!(
            effective_qps = INITIAL_QPS,
            "Batch write flow control: admission initialized"
        );
        controller
    }

    pub(crate) async fn admit_rpc(&self) -> WriteAdmission<'_> {
        let mut total_wait = Duration::ZERO;
        loop {
            let (completed_observation, reservation) = {
                let mut state = self
                    .state
                    .lock()
                    .expect("flow-control state mutex poisoned");
                let now = Instant::now();
                state.restart_or_close_empty_observation_if_ready(now);
                // An immediately available permit belongs to this window; finish after recording
                // its admitted start.
                let completed_observation = if state.next_permit_at <= now {
                    None
                } else {
                    Self::finish_observation_if_ready(&mut state, now)
                };
                let reservation = state.reserve_permit(now);
                (completed_observation, reservation)
            };
            if let Some(completed_observation) = completed_observation {
                self.emit_observation_completed_telemetry(completed_observation);
            }
            total_wait = total_wait.saturating_add(reservation.wait);
            if !reservation.wait.is_zero() {
                tokio::time::sleep(reservation.wait).await;
            }

            let (completed_observation, admitted) = {
                let mut state = self
                    .state
                    .lock()
                    .expect("flow-control state mutex poisoned");
                let now = Instant::now();
                // Restart an empty observation before recording a valid start so idle time is
                // excluded. Finish afterward so a boundary start counts toward utilization.
                state.restart_or_close_empty_observation_if_ready(now);
                let admitted = state.complete_reservation(reservation);
                let completed_observation = Self::finish_observation_if_ready(&mut state, now);
                (completed_observation, admitted)
            };
            if let Some(completed_observation) = completed_observation {
                self.emit_observation_completed_telemetry(completed_observation);
            }
            if admitted {
                if let Some(metrics) = &self.metrics {
                    metrics
                        .kv_bt_flow_control_throttle_ms
                        .with_label_values(&[&self.client_name])
                        .observe(total_wait.as_secs_f64() * 1_000.0);
                }
                return WriteAdmission {
                    flow_controller: self,
                    started_at: Instant::now(),
                    rate_generation: reservation.rate_generation,
                };
            }
        }
    }

    fn finish_observation_if_ready(
        state: &mut ControllerState,
        now: Instant,
    ) -> Option<CompletedObservation> {
        let observation = state.observation.as_ref()?;
        let elapsed = now.saturating_duration_since(observation.started_at);
        if elapsed < OBSERVATION_WINDOW {
            return None;
        }
        if observation.rpc_starts == 0 {
            state.restart_or_close_empty_observation_if_ready(now);
            return None;
        }

        let observed_start_qps = observation.rpc_starts as f64 / elapsed.as_secs_f64();
        let current_qps = state.effective_qps;
        let utilization = observed_start_qps / current_qps;
        let factor = state
            .pending
            .combined_factor()
            .expect("an observation requires pending feedback");
        let reanchor_to_observed = state.pending.reanchors_to_observed();
        // Growth compounds from the current limit rather than the sampled start rate so pacing
        // jitter cannot lower the limit when utilization is sufficient.
        let effective_qps = if reanchor_to_observed {
            (observed_start_qps * factor)
                .clamp(MIN_QPS, MAX_QPS)
                .min(current_qps)
        } else if utilization >= UPWARD_UTILIZATION_THRESHOLD {
            state.pending.growth_factor().map_or(current_qps, |factor| {
                (current_qps * factor).clamp(MIN_QPS, MAX_QPS)
            })
        } else {
            current_qps
        };
        if effective_qps != state.effective_qps {
            state.effective_qps = effective_qps;
            state.rate_generation = state.rate_generation.saturating_add(1);
            state.latency_total_micros = 0;
            state.latency_samples = 0;
            state.latency_sampling_started_at = None;
            state.next_latency_evaluation_at = now + LATENCY_EVALUATION_PERIOD;
            state.next_permit_at = now;
        }
        if state.pending.server_factor.is_some() {
            state.next_server_update_at =
                now + state.pending.server_period.unwrap_or(DEFAULT_PERIOD);
        }
        state.observation = None;
        state.pending = PendingFeedback::default();

        Some(CompletedObservation {
            observed_start_qps,
            effective_qps,
        })
    }

    fn emit_observation_completed_telemetry(&self, completed_observation: CompletedObservation) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_effective_qps
                .with_label_values(&[&self.client_name])
                .set(completed_observation.effective_qps);
            metrics
                .kv_bt_flow_control_last_observation_start_qps
                .with_label_values(&[&self.client_name])
                .set(completed_observation.observed_start_qps);
        }
        self.increment_flow_control_event("observation_completed");
        info!(
            observed_start_qps = completed_observation.observed_start_qps,
            effective_qps = completed_observation.effective_qps,
            "Batch write flow control: feedback observation completed"
        );
    }

    fn on_server_feedback(&self, info: Option<&RateLimitInfo>) {
        let Some(feedback) = Self::validated_server_feedback(info) else {
            return;
        };
        let feedback_direction = feedback.direction_label();
        let (completed_observation, started_observation, server_feedback_rejected) = {
            let mut state = self
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            let now = Instant::now();
            let completed_observation = Self::finish_observation_if_ready(&mut state, now);
            let server_feedback_allowed = now >= state.next_server_update_at;
            let healthy_growth_pending = state.pending.allows_growth();
            let (started_observation, server_feedback_rejected) =
                if server_feedback_allowed && (feedback.factor <= 1.0 || healthy_growth_pending) {
                    (
                        Self::queue_server_feedback(&mut state, now, feedback),
                        false,
                    )
                } else {
                    (false, true)
                };
            (
                completed_observation,
                started_observation,
                server_feedback_rejected,
            )
        };
        if let Some(completed_observation) = completed_observation {
            self.emit_observation_completed_telemetry(completed_observation);
        }
        if started_observation {
            self.emit_observation_started_telemetry();
        }
        self.increment_server_feedback(
            feedback_direction,
            if server_feedback_rejected {
                "rejected"
            } else {
                "queued"
            },
        );
        if server_feedback_rejected {
            self.emit_server_feedback_rejected_telemetry();
        }
    }

    fn complete_error(&self, code: Code) {
        if !is_overload_error(code) {
            return;
        }

        let (completed_observation, started_observation, server_feedback_rejected) = {
            let mut state = self
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            let now = Instant::now();
            let completed_observation = Self::finish_observation_if_ready(&mut state, now);
            let server_feedback_allowed = now >= state.next_server_update_at;
            let (started_observation, server_feedback_rejected) = if server_feedback_allowed {
                (
                    Self::queue_server_feedback(
                        &mut state,
                        now,
                        ServerFeedback {
                            factor: MIN_FACTOR,
                            period: DEFAULT_PERIOD,
                        },
                    ),
                    false,
                )
            } else {
                (false, true)
            };
            (
                completed_observation,
                started_observation,
                server_feedback_rejected,
            )
        };
        if let Some(completed_observation) = completed_observation {
            self.emit_observation_completed_telemetry(completed_observation);
        }
        if started_observation {
            self.emit_observation_started_telemetry();
        }
        if server_feedback_rejected {
            self.emit_server_feedback_rejected_telemetry();
        }
    }

    fn validated_server_feedback(info: Option<&RateLimitInfo>) -> Option<ServerFeedback> {
        let info = info?;
        if !info.factor.is_finite() || info.factor <= 0.0 {
            return None;
        }
        let period = info.period.as_ref()?;
        if period.seconds < 0 || !(0..1_000_000_000).contains(&period.nanos) {
            return None;
        }
        let period = Duration::new(period.seconds as u64, period.nanos as u32);
        if period.is_zero() {
            return None;
        }
        Some(ServerFeedback {
            factor: info.factor.clamp(MIN_FACTOR, MAX_FACTOR),
            period,
        })
    }

    fn queue_server_feedback(
        state: &mut ControllerState,
        now: Instant,
        feedback: ServerFeedback,
    ) -> bool {
        let ServerFeedback { factor, period } = feedback;
        let started_observation = state.observation.is_none();
        state.pending.server_factor = Some(
            state
                .pending
                .server_factor
                .map_or(factor, |pending| pending.min(factor)),
        );
        state.pending.server_period = Some(
            state
                .pending
                .server_period
                .map_or(period, |pending| pending.max(period)),
        );
        state.observation.get_or_insert(ObservationWindow {
            started_at: now,
            rpc_starts: 0,
        });
        started_observation
    }

    fn queue_latency_feedback(
        state: &mut ControllerState,
        now: Instant,
        feedback: LatencyFeedback,
    ) -> bool {
        let started_observation = state.observation.is_none();
        state.pending.latency_feedback = Some(
            state
                .pending
                .latency_feedback
                .map_or(feedback, |pending| pending.more_conservative(feedback)),
        );
        state.observation.get_or_insert(ObservationWindow {
            started_at: now,
            rpc_starts: 0,
        });
        started_observation
    }

    fn emit_observation_started_telemetry(&self) {
        self.increment_flow_control_event("observation_started");
        info!("Batch write flow control: feedback observation started");
    }

    fn increment_server_feedback(&self, direction: &str, outcome: &str) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_server_feedback_total
                .with_label_values(&[self.client_name.as_str(), direction, outcome])
                .inc();
        }
    }

    fn increment_latency_feedback(&self, feedback: LatencyFeedback) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_latency_feedback_total
                .with_label_values(&[self.client_name.as_str(), feedback.direction_label()])
                .inc();
        }
    }

    fn emit_server_feedback_rejected_telemetry(&self) {
        self.increment_flow_control_event("feedback_rejected");
    }

    fn increment_flow_control_event(&self, event: &str) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_events_total
                .with_label_values(&[self.client_name.as_str(), event])
                .inc();
        }
    }

    fn evaluate_latency_if_ready(
        state: &mut ControllerState,
        now: Instant,
    ) -> Option<LatencyEvaluation> {
        if now < state.next_latency_evaluation_at || state.latency_samples < MIN_WINDOW_SAMPLES {
            return None;
        }

        let window_avg_micros = state.latency_total_micros as f64 / state.latency_samples as f64;
        let evidence_duration = now
            .saturating_duration_since(
                state
                    .latency_sampling_started_at
                    .take()
                    .expect("latency samples missing sampling start"),
            )
            .clamp(LATENCY_EVALUATION_PERIOD, state.baseline_evidence_cap());
        state.latency_total_micros = 0;
        state.latency_samples = 0;
        state.next_latency_evaluation_at = now + LATENCY_EVALUATION_PERIOD;

        let condition =
            classify_write_latency(window_avg_micros, state.baseline_write_latency_micros);
        if matches!(
            condition,
            WriteLatencyCondition::Elevated | WriteLatencyCondition::Severe
        ) && let Some(baseline) = state.baseline_write_latency_micros.as_mut()
        {
            let alpha = non_healthy_baseline_alpha(evidence_duration);
            *baseline += alpha * (window_avg_micros - *baseline);
        }
        let latency_feedback = match condition {
            WriteLatencyCondition::Severe => Some(LatencyFeedback::Decrease),
            WriteLatencyCondition::Elevated => Some(LatencyFeedback::Reanchor),
            WriteLatencyCondition::Healthy => {
                let had_baseline = state.baseline_write_latency_micros.is_some();
                let baseline = state
                    .baseline_write_latency_micros
                    .get_or_insert(window_avg_micros);
                *baseline += BASELINE_EWMA_ALPHA * (window_avg_micros - *baseline);
                had_baseline.then_some(LatencyFeedback::Increase)
            }
        };
        let started_observation = latency_feedback
            .is_some_and(|feedback| Self::queue_latency_feedback(state, now, feedback));

        Some(LatencyEvaluation {
            window_avg_micros,
            baseline_micros: state.baseline_write_latency_micros,
            condition,
            feedback: latency_feedback,
            started_observation,
        })
    }

    fn complete_stream(&self, rate_generation: u64, elapsed: Duration) {
        let (completed_observation, latency_evaluation) = {
            let mut state = self
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            let now = Instant::now();
            let completed_observation = Self::finish_observation_if_ready(&mut state, now);
            let latency_evaluation = if rate_generation != state.rate_generation {
                None
            } else {
                let elapsed_micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
                if state.latency_samples == 0 {
                    state.latency_sampling_started_at = Some(now);
                }
                state.latency_total_micros =
                    state.latency_total_micros.saturating_add(elapsed_micros);
                state.latency_samples = state.latency_samples.saturating_add(1);
                Self::evaluate_latency_if_ready(&mut state, now)
            };
            (completed_observation, latency_evaluation)
        };

        if let Some(completed_observation) = completed_observation {
            self.emit_observation_completed_telemetry(completed_observation);
        }
        if latency_evaluation
            .as_ref()
            .is_some_and(|evaluation| evaluation.started_observation)
        {
            self.emit_observation_started_telemetry();
        }
        if let Some(latency_evaluation) = latency_evaluation {
            if let Some(metrics) = &self.metrics {
                metrics
                    .kv_bt_flow_control_write_latency_window_avg_ms
                    .with_label_values(&[&self.client_name])
                    .set(latency_evaluation.window_avg_micros / 1_000.0);
                if let Some(baseline_micros) = latency_evaluation.baseline_micros {
                    metrics
                        .kv_bt_flow_control_write_latency_baseline_ms
                        .with_label_values(&[&self.client_name])
                        .set(baseline_micros / 1_000.0);
                }
            }
            if let Some(feedback) = latency_evaluation.feedback {
                self.increment_latency_feedback(feedback);
            }
            if latency_evaluation.condition == WriteLatencyCondition::Severe {
                info!(
                    window_avg_ms = latency_evaluation.window_avg_micros / 1_000.0,
                    "Batch write flow control: severe write latency observed"
                );
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn effective_qps(&self) -> f64 {
        self.state
            .lock()
            .expect("flow-control state mutex poisoned")
            .effective_qps
    }
}

impl WriteAdmission<'_> {
    pub(crate) fn on_server_feedback(&self, info: Option<&RateLimitInfo>) {
        self.flow_controller.on_server_feedback(info);
    }

    pub(crate) fn complete(self) {
        self.flow_controller
            .complete_stream(self.rate_generation, self.started_at.elapsed());
    }

    pub(crate) fn fail(self, code: Code) {
        self.flow_controller.complete_error(code);
    }
}

#[cfg(test)]
mod tests {
    use prometheus::Registry;

    use super::*;

    fn rate_limit_info(factor: f64, period: Duration) -> RateLimitInfo {
        raw_rate_limit_info(
            factor,
            period.as_secs() as i64,
            period.subsec_nanos() as i32,
        )
    }

    fn raw_rate_limit_info(factor: f64, seconds: i64, nanos: i32) -> RateLimitInfo {
        RateLimitInfo {
            period: Some(prost_types::Duration { seconds, nanos }),
            factor,
        }
    }

    fn assert_effective_qps(controller: &BatchWriteFlowController, expected: f64) {
        let actual = controller.effective_qps();
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected} QPS, got {actual}"
        );
    }

    fn set_effective_qps_fixture(controller: &BatchWriteFlowController, effective_qps: f64) {
        let mut state = controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned");
        state.effective_qps = effective_qps;
        state.next_permit_at = Instant::now();
    }

    #[derive(Debug, PartialEq)]
    struct ControllerSnapshot {
        effective_qps: f64,
        observation: Option<(Instant, u64)>,
        server_factor: Option<f64>,
        latency_feedback: Option<LatencyFeedback>,
        server_period: Option<Duration>,
    }

    fn observation_snapshot(controller: &BatchWriteFlowController) -> ControllerSnapshot {
        let state = controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned");
        ControllerSnapshot {
            effective_qps: state.effective_qps,
            observation: state
                .observation
                .as_ref()
                .map(|window| (window.started_at, window.rpc_starts)),
            server_factor: state.pending.server_factor,
            latency_feedback: state.pending.latency_feedback,
            server_period: state.pending.server_period,
        }
    }

    fn pending_latency_feedback(controller: &BatchWriteFlowController) -> Option<LatencyFeedback> {
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .pending
            .latency_feedback
    }

    fn baseline_micros(controller: &BatchWriteFlowController) -> Option<f64> {
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .baseline_write_latency_micros
    }

    fn latency_samples(controller: &BatchWriteFlowController) -> u64 {
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .latency_samples
    }

    fn make_latency_evaluation_ready(controller: &BatchWriteFlowController) {
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .next_latency_evaluation_at = Instant::now();
    }

    fn current_admission(controller: &BatchWriteFlowController) -> WriteAdmission<'_> {
        let rate_generation = controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .rate_generation;
        WriteAdmission {
            flow_controller: controller,
            started_at: Instant::now(),
            rate_generation,
        }
    }

    fn admission_with_elapsed(
        controller: &BatchWriteFlowController,
        elapsed: Duration,
    ) -> WriteAdmission<'_> {
        let mut admission = current_admission(controller);
        admission.started_at = Instant::now()
            .checked_sub(elapsed)
            .expect("paused test clock should allow the requested latency");
        admission
    }

    fn fail_admission(controller: &BatchWriteFlowController, code: Code) {
        current_admission(controller).fail(code);
    }

    fn record_latency_samples(
        controller: &BatchWriteFlowController,
        samples: u64,
        latency: Duration,
    ) {
        for _ in 0..samples {
            admission_with_elapsed(controller, latency).complete();
        }
    }

    async fn record_latency_window_over(
        controller: &BatchWriteFlowController,
        latency: Duration,
        evidence_duration: Duration,
    ) {
        record_latency_samples(controller, 1, latency);
        tokio::time::advance(evidence_duration).await;
        record_latency_samples(controller, MIN_WINDOW_SAMPLES - 1, latency);
    }

    async fn complete_rpc_with_latency(
        controller: &Arc<BatchWriteFlowController>,
        rpc_latency: Duration,
    ) -> (Instant, ControllerSnapshot) {
        let admission = controller.admit_rpc().await;
        let admission_snapshot = (Instant::now(), observation_snapshot(controller));
        tokio::time::advance(rpc_latency).await;
        admission.complete();
        admission_snapshot
    }

    fn learn_healthy_baseline(controller: &BatchWriteFlowController) {
        make_latency_evaluation_ready(controller);
        record_latency_samples(controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        assert_eq!(baseline_micros(controller), Some(20_000.0));
        assert_eq!(pending_latency_feedback(controller), None);
    }

    async fn admit_rpcs(controller: &Arc<BatchWriteFlowController>, count: usize) {
        for _ in 0..count {
            drop(controller.admit_rpc().await);
        }
    }

    async fn advance_observation_to_boundary(controller: &BatchWriteFlowController) {
        let started_at = controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .observation
            .as_ref()
            .expect("expected an active observation")
            .started_at;
        let elapsed = Instant::now().saturating_duration_since(started_at);
        if elapsed < OBSERVATION_WINDOW {
            tokio::time::advance(OBSERVATION_WINDOW - elapsed).await;
        }
    }

    fn finish_ready_observation(controller: &BatchWriteFlowController) {
        let completed_observation = {
            let mut state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            BatchWriteFlowController::finish_observation_if_ready(&mut state, Instant::now())
        };
        if let Some(completed_observation) = completed_observation {
            controller.emit_observation_completed_telemetry(completed_observation);
        }
    }

    async fn finish_observation(controller: &BatchWriteFlowController) {
        advance_observation_to_boundary(controller).await;
        finish_ready_observation(controller);
    }

    fn finish_observation_with_starts(controller: &BatchWriteFlowController, rpc_starts: u64) {
        let now = Instant::now();
        {
            let mut state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            let observation = state
                .observation
                .as_mut()
                .expect("expected an active observation");
            observation.started_at = now
                .checked_sub(OBSERVATION_WINDOW)
                .expect("paused test clock should allow a one-second lookback");
            observation.rpc_starts = rpc_starts;
        }
        finish_ready_observation(controller);
    }

    fn apply_ready_server_decrease(controller: &BatchWriteFlowController) {
        let now = Instant::now();
        {
            let mut state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            state.observation = Some(ObservationWindow {
                started_at: now
                    .checked_sub(OBSERVATION_WINDOW)
                    .expect("paused test clock should allow a one-second lookback"),
                rpc_starts: INITIAL_QPS as u64,
            });
            state.pending.server_factor = Some(MIN_FACTOR);
            state.pending.server_period = Some(DEFAULT_PERIOD);
        }
        finish_ready_observation(controller);
    }

    fn flow_control_event_count(metrics: &KvMetrics, client: &str, event: &str) -> u64 {
        metrics
            .kv_bt_flow_control_events_total
            .with_label_values(&[client, event])
            .get()
    }

    fn server_feedback_count(
        metrics: &KvMetrics,
        client: &str,
        direction: &str,
        outcome: &str,
    ) -> u64 {
        metrics
            .kv_bt_flow_control_server_feedback_total
            .with_label_values(&[client, direction, outcome])
            .get()
    }

    fn latency_feedback_count(metrics: &KvMetrics, client: &str, direction: &str) -> u64 {
        metrics
            .kv_bt_flow_control_latency_feedback_total
            .with_label_values(&[client, direction])
            .get()
    }

    #[tokio::test(start_paused = true)]
    async fn starts_at_initial_rate_and_records_only_admitted_rpcs() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        assert_effective_qps(&controller, INITIAL_QPS);
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));

        drop(controller.admit_rpc().await);
        let second_admission = tokio::spawn({
            let controller = controller.clone();
            async move {
                drop(controller.admit_rpc().await);
            }
        });
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_millis(99)).await;
        tokio::task::yield_now().await;
        assert!(!second_admission.is_finished());
        assert_eq!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .as_ref()
                .expect("feedback should open an observation")
                .rpc_starts,
            1
        );

        tokio::time::advance(Duration::from_millis(1)).await;
        second_admission.await.unwrap();
        assert_eq!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .as_ref()
                .expect("feedback should open an observation")
                .rpc_starts,
            2
        );
    }

    #[tokio::test(start_paused = true)]
    async fn server_decrease_waits_for_complete_bootstrap_observation() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
        assert_effective_qps(&controller, INITIAL_QPS);

        admit_rpcs(&controller, 10).await;
        assert_effective_qps(&controller, INITIAL_QPS);
        finish_observation(&controller).await;

        assert_effective_qps(&controller, 7.0);
    }

    #[tokio::test(start_paused = true)]
    async fn server_factor_is_clamped_when_queued() {
        let lower = BatchWriteFlowController::new("lower".to_owned(), None);
        lower.on_server_feedback(Some(&rate_limit_info(0.3, DEFAULT_PERIOD)));
        assert_eq!(
            lower
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .pending
                .server_factor,
            Some(MIN_FACTOR)
        );

        let upper = BatchWriteFlowController::new("upper".to_owned(), None);
        learn_healthy_baseline(&upper);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&upper, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        upper.on_server_feedback(Some(&rate_limit_info(2.0, DEFAULT_PERIOD)));
        assert_eq!(
            upper
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .pending
                .server_factor,
            Some(MAX_FACTOR)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn server_factor_application_is_bounded_by_min_qps() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller.on_server_feedback(Some(&rate_limit_info(0.3, DEFAULT_PERIOD)));
        drop(controller.admit_rpc().await);

        tokio::time::advance(Duration::from_secs(10)).await;
        finish_ready_observation(&controller);

        assert_effective_qps(&controller, MIN_QPS);
    }

    #[tokio::test(start_paused = true)]
    async fn server_factor_application_is_bounded_by_max_qps() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        set_effective_qps_fixture(&controller, MAX_QPS * 0.9);
        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        controller.on_server_feedback(Some(&rate_limit_info(2.0, DEFAULT_PERIOD)));

        finish_observation_with_starts(&controller, (MAX_QPS * 0.9) as u64);

        assert_effective_qps(&controller, MAX_QPS);
    }

    #[tokio::test(start_paused = true)]
    async fn pending_feedback_uses_most_restrictive_factor() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller.on_server_feedback(Some(&rate_limit_info(1.0, Duration::from_secs(1))));
        fail_admission(&controller, Code::Unavailable);
        {
            let state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            assert_eq!(state.pending.server_factor, Some(MIN_FACTOR));
            assert_eq!(state.pending.server_period, Some(DEFAULT_PERIOD));
        }

        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;

        assert_effective_qps(&controller, 7.0);
    }

    #[tokio::test(start_paused = true)]
    async fn upward_server_feedback_requires_fresh_healthy_latency() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller.on_server_feedback(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert_effective_qps(&controller, INITIAL_QPS);
        assert!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .is_none()
        );

        learn_healthy_baseline(&controller);
        controller.on_server_feedback(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .is_none()
        );

        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        controller.on_server_feedback(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;

        assert_effective_qps(&controller, INITIAL_QPS * MAX_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn healthy_growth_multiplies_current_limit_not_observed_starts() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));

        finish_observation_with_starts(&controller, 9);

        assert_effective_qps(&controller, INITIAL_QPS * HEALTHY_RECOVERY_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn healthy_growth_requires_eighty_percent_utilization() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));

        finish_observation_with_starts(&controller, 7);
        assert_effective_qps(&controller, INITIAL_QPS);

        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        finish_observation_with_starts(&controller, 8);
        assert_effective_qps(&controller, INITIAL_QPS * HEALTHY_RECOVERY_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn immediate_boundary_start_counts_toward_growth_utilization() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        let starts_required_for_growth =
            (INITIAL_QPS * OBSERVATION_WINDOW.as_secs_f64() * UPWARD_UTILIZATION_THRESHOLD).ceil()
                as u64;
        let now = Instant::now();
        {
            let mut state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            state.observation = Some(ObservationWindow {
                started_at: now
                    .checked_sub(OBSERVATION_WINDOW)
                    .expect("paused test clock should allow a one-second lookback"),
                rpc_starts: starts_required_for_growth - 1,
            });
            state.pending.latency_feedback = Some(LatencyFeedback::Increase);
        }

        drop(controller.admit_rpc().await);

        assert_effective_qps(&controller, INITIAL_QPS * HEALTHY_RECOVERY_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn neutral_server_feedback_does_not_lower_rate_from_observation_undercount() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller.on_server_feedback(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        finish_observation_with_starts(&controller, 9);

        assert_effective_qps(&controller, INITIAL_QPS);
    }

    #[tokio::test(start_paused = true)]
    async fn zero_start_window_retains_pending_feedback() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        set_effective_qps_fixture(&controller, 50.0);
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));

        tokio::time::advance(OBSERVATION_WINDOW).await;
        finish_ready_observation(&controller);
        assert_effective_qps(&controller, 50.0);
        {
            let state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            assert_eq!(state.pending.server_factor, Some(MIN_FACTOR));
            assert_eq!(
                state
                    .observation
                    .as_ref()
                    .expect("zero-start feedback should restart its observation")
                    .rpc_starts,
                0
            );
        }

        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 7.0);
    }

    #[tokio::test(start_paused = true)]
    async fn conservative_feedback_excludes_idle_before_first_admitted_start() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
        tokio::time::advance(Duration::from_secs(60 * 60)).await;

        let admitted_at = Instant::now();
        drop(controller.admit_rpc().await);

        assert_effective_qps(&controller, INITIAL_QPS);
        let snapshot = observation_snapshot(&controller);
        assert_eq!(snapshot.observation, Some((admitted_at, 1)));
        assert_eq!(snapshot.server_factor, Some(MIN_FACTOR));
    }

    #[tokio::test(start_paused = true)]
    async fn healthy_growth_feedback_expires_after_rate_aware_empty_window() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        controller.on_server_feedback(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));

        let growth_observation_timeout = controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .growth_observation_timeout();
        assert!(growth_observation_timeout > OBSERVATION_WINDOW);

        tokio::time::advance(OBSERVATION_WINDOW).await;
        finish_ready_observation(&controller);
        {
            let state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            assert!(state.observation.is_some());
            assert_eq!(
                state.pending.latency_feedback,
                Some(LatencyFeedback::Increase)
            );
            assert_eq!(state.pending.server_factor, Some(MAX_FACTOR));
        }

        tokio::time::advance(growth_observation_timeout - OBSERVATION_WINDOW).await;
        finish_ready_observation(&controller);
        {
            let state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            assert!(state.observation.is_none());
            assert!(state.pending.latency_feedback.is_none());
            assert!(state.pending.server_factor.is_none());
        }
        assert_effective_qps(&controller, INITIAL_QPS);

        controller.on_server_feedback(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        assert!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn conservative_server_feedback_restarts_empty_growth_observation() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));

        {
            let state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            assert_eq!(
                state.pending.latency_feedback,
                Some(LatencyFeedback::Increase)
            );
            assert_eq!(state.pending.server_factor, Some(MIN_FACTOR));
            assert!(state.pending.growth_factor().is_none());
        }

        tokio::time::advance(OBSERVATION_WINDOW).await;
        finish_ready_observation(&controller);
        {
            let state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            assert!(state.pending.latency_feedback.is_none());
            assert_eq!(state.pending.server_factor, Some(MIN_FACTOR));
            assert_eq!(
                state
                    .observation
                    .as_ref()
                    .expect("conservative feedback should restart its observation")
                    .rpc_starts,
                0
            );
        }

        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, INITIAL_QPS * MIN_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn min_qps_recovers_under_sustained_healthy_load() {
        const STABLE_RPC_LATENCY: Duration = Duration::from_millis(200);
        const MAX_COMPLETED_RPCS: u64 = 30;
        const MAX_RECOVERY_TIME: Duration = Duration::from_secs(5 * 60);

        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        // Matching the safe baseline isolates feedback liveness from stale-baseline adaptation.
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .baseline_write_latency_micros = Some(200_000.0);
        set_effective_qps_fixture(&controller, MIN_QPS);

        let simulation_started_at = Instant::now();
        let deadline = simulation_started_at + MAX_RECOVERY_TIME;
        let mut completed_rpcs = 0;
        let mut saw_growth_feedback = false;
        let mut saw_growth_discarded = false;

        while controller.effective_qps() <= MIN_QPS
            && completed_rpcs < MAX_COMPLETED_RPCS
            && Instant::now() < deadline
        {
            let pre_rpc_snapshot = observation_snapshot(&controller);
            let zero_start_growth_observation_started_at = if pre_rpc_snapshot.latency_feedback
                == Some(LatencyFeedback::Increase)
            {
                saw_growth_feedback = true;
                pre_rpc_snapshot
                    .observation
                    .and_then(|(started_at, rpc_starts)| (rpc_starts == 0).then_some(started_at))
            } else {
                None
            };

            let completed_rpc = tokio::time::timeout_at(
                deadline,
                complete_rpc_with_latency(&controller, STABLE_RPC_LATENCY),
            )
            .await;
            let Ok((admitted_at, admission_snapshot)) = completed_rpc else {
                break;
            };
            completed_rpcs += 1;

            if zero_start_growth_observation_started_at.is_some_and(|observation_started_at| {
                admitted_at.saturating_duration_since(observation_started_at) >= OBSERVATION_WINDOW
                    && admission_snapshot.effective_qps <= MIN_QPS
                    && admission_snapshot.latency_feedback != Some(LatencyFeedback::Increase)
                    && admission_snapshot.observation.is_none()
            }) {
                saw_growth_discarded = true;
            }

            let post_completion_snapshot = observation_snapshot(&controller);
            if post_completion_snapshot.latency_feedback == Some(LatencyFeedback::Increase) {
                saw_growth_feedback = true;
            }
        }

        let effective_qps = controller.effective_qps();
        if effective_qps <= MIN_QPS {
            assert!(
                completed_rpcs >= MIN_WINDOW_SAMPLES,
                "expected at least {MIN_WINDOW_SAMPLES} completed RPCs, got {completed_rpcs}"
            );
            assert!(
                saw_growth_feedback,
                "expected healthy latency to authorize growth"
            );
            assert!(
                saw_growth_discarded,
                "expected sparse admission to discard authorized growth"
            );
        }
        assert!(
            effective_qps > MIN_QPS,
            "expected recovery above {MIN_QPS} QPS after sustained healthy demand; completed_rpcs={completed_rpcs}, saw_growth_feedback={saw_growth_feedback}, saw_growth_discarded={saw_growth_discarded}, effective_qps={effective_qps}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn sustained_queued_writes_with_positive_feedback_recover_to_initial_qps() {
        const STABLE_RPC_LATENCY: Duration = Duration::from_millis(20);
        const MAX_COMPLETED_RPCS: u64 = 10_000;
        const RECOVERY_DEADLINE: Duration = Duration::from_secs(60 * 60);

        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .baseline_write_latency_micros = Some(STABLE_RPC_LATENCY.as_micros() as f64);
        set_effective_qps_fixture(&controller, MIN_QPS);

        let affirmative_feedback = rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD);
        let simulation_started_at = Instant::now();
        let deadline = simulation_started_at + RECOVERY_DEADLINE;
        let mut completed_rpcs = 0;

        while controller.effective_qps() < INITIAL_QPS
            && completed_rpcs < MAX_COMPLETED_RPCS
            && Instant::now() < deadline
        {
            let completed_rpc = tokio::time::timeout_at(deadline, async {
                let admission = controller.admit_rpc().await;
                tokio::time::advance(STABLE_RPC_LATENCY).await;
                admission.on_server_feedback(Some(&affirmative_feedback));
                admission.complete();
            })
            .await;
            if completed_rpc.is_err() {
                break;
            }
            completed_rpcs += 1;
        }

        let elapsed = Instant::now().saturating_duration_since(simulation_started_at);
        let final_effective_qps = controller.effective_qps();
        assert!(
            final_effective_qps >= INITIAL_QPS,
            "expected sustained healthy writes to recover from {MIN_QPS} to at least {INITIAL_QPS} QPS; completed_rpcs={completed_rpcs}, elapsed={elapsed:?}, deadline={RECOVERY_DEADLINE:?}, completion_bound={MAX_COMPLETED_RPCS}, final_effective_qps={final_effective_qps}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stale_latency_baseline_recovers_at_min_qps_within_deadline() {
        const LEARNED_BASELINE: Duration = Duration::from_millis(200);
        const STABLE_RPC_LATENCY: Duration = Duration::from_millis(400);
        const MAX_COMPLETED_RPCS: u64 = 1_000;
        const RECOVERY_DEADLINE: Duration = Duration::from_secs(15 * 60);

        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        // Direct state setup isolates recovery after a historical safe latency has changed.
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .baseline_write_latency_micros = Some(LEARNED_BASELINE.as_micros() as f64);
        set_effective_qps_fixture(&controller, MIN_QPS);

        let deadline = Instant::now() + RECOVERY_DEADLINE;
        let mut completed_rpcs = 0;
        let mut saw_recorded_latency_sample = false;
        let mut saw_growth_feedback = false;

        while controller.effective_qps() <= MIN_QPS
            && completed_rpcs < MAX_COMPLETED_RPCS
            && Instant::now() < deadline
        {
            saw_growth_feedback |=
                pending_latency_feedback(&controller) == Some(LatencyFeedback::Increase);
            let completed_rpc = tokio::time::timeout_at(
                deadline,
                complete_rpc_with_latency(&controller, STABLE_RPC_LATENCY),
            )
            .await;
            let Ok((_, admission_snapshot)) = completed_rpc else {
                break;
            };
            completed_rpcs += 1;

            saw_growth_feedback |=
                admission_snapshot.latency_feedback == Some(LatencyFeedback::Increase);
            saw_recorded_latency_sample |= latency_samples(&controller) > 0;
            saw_growth_feedback |=
                pending_latency_feedback(&controller) == Some(LatencyFeedback::Increase);
        }

        let effective_qps = controller.effective_qps();
        if effective_qps <= MIN_QPS {
            assert!(
                completed_rpcs >= MIN_WINDOW_SAMPLES,
                "expected completed latency samples under sustained demand; completed_rpcs={completed_rpcs}"
            );
            assert!(
                saw_recorded_latency_sample,
                "expected completed RPC latency to enter the evaluation window"
            );
        }
        assert!(
            effective_qps > MIN_QPS,
            "expected recovery above {MIN_QPS} QPS within 15 virtual minutes after stable latency changed from 200 ms to 400 ms; completed_rpcs={completed_rpcs}, baseline_micros={:?}, saw_recorded_latency_sample={saw_recorded_latency_sample}, saw_growth_feedback={saw_growth_feedback}, effective_qps={effective_qps}",
            baseline_micros(&controller)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn severe_latency_with_positive_server_feedback_recovers_at_min_qps_within_deadline() {
        const LEARNED_BASELINE: Duration = Duration::from_millis(200);
        const STABLE_RPC_LATENCY: Duration = Duration::from_millis(1_200);
        const MAX_COMPLETED_RPCS: u64 = 2_000;
        const RECOVERY_DEADLINE: Duration = Duration::from_secs(20 * 60);

        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        controller
            .state
            .lock()
            .expect("flow-control state mutex poisoned")
            .baseline_write_latency_micros = Some(LEARNED_BASELINE.as_micros() as f64);
        set_effective_qps_fixture(&controller, MIN_QPS);

        let affirmative_feedback = rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD);
        let deadline = Instant::now() + RECOVERY_DEADLINE;
        let mut completed_rpcs = 0;
        let mut positive_server_feedback_attempts = 0;
        let mut saw_recorded_latency_sample = false;
        let mut saw_severe_feedback = false;

        while controller.effective_qps() <= MIN_QPS
            && completed_rpcs < MAX_COMPLETED_RPCS
            && Instant::now() < deadline
        {
            let completed_rpc = tokio::time::timeout_at(deadline, async {
                let admission = controller.admit_rpc().await;
                tokio::time::advance(STABLE_RPC_LATENCY).await;
                admission.on_server_feedback(Some(&affirmative_feedback));
                admission.complete();
                (
                    latency_samples(&controller),
                    pending_latency_feedback(&controller),
                )
            })
            .await;
            let Ok((recorded_samples, latency_feedback)) = completed_rpc else {
                break;
            };

            completed_rpcs += 1;
            positive_server_feedback_attempts += 1;
            saw_recorded_latency_sample |= recorded_samples > 0;
            saw_severe_feedback |= latency_feedback == Some(LatencyFeedback::Decrease);
        }

        let effective_qps = controller.effective_qps();
        let final_baseline_micros = baseline_micros(&controller);
        if effective_qps <= MIN_QPS {
            assert!(
                completed_rpcs >= MIN_WINDOW_SAMPLES && saw_recorded_latency_sample,
                "missing completed latency samples under sustained demand; completed_rpcs={completed_rpcs}, saw_recorded_latency_sample={saw_recorded_latency_sample}"
            );
            assert!(
                positive_server_feedback_attempts >= MIN_WINDOW_SAMPLES,
                "missing repeated positive server feedback attempts; positive_server_feedback_attempts={positive_server_feedback_attempts}, completed_rpcs={completed_rpcs}"
            );
            assert!(
                saw_severe_feedback,
                "expected stable 1.2 second RPC latency to exercise Severe recovery from a 200 ms baseline"
            );
            assert_ne!(
                final_baseline_micros,
                Some(LEARNED_BASELINE.as_micros() as f64),
                "latency baseline remained unchanged despite completed Severe samples and repeated positive server feedback attempts; completed_rpcs={completed_rpcs}, positive_server_feedback_attempts={positive_server_feedback_attempts}"
            );
        }
        assert!(
            effective_qps > MIN_QPS,
            "no growth above {MIN_QPS} QPS within 20 virtual minutes; completed_rpcs={completed_rpcs}, positive_server_feedback_attempts={positive_server_feedback_attempts}, saw_recorded_latency_sample={saw_recorded_latency_sample}, saw_severe_feedback={saw_severe_feedback}, baseline_micros={final_baseline_micros:?}, effective_qps={effective_qps}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn late_feedback_starts_a_new_observation_after_server_period() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        let period = Duration::from_secs(1);
        controller.on_server_feedback(Some(&rate_limit_info(1.0, period)));
        admit_rpcs(&controller, 10).await;

        advance_observation_to_boundary(&controller).await;
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, period)));
        assert_effective_qps(&controller, INITIAL_QPS);
        assert!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .is_none()
        );

        tokio::time::advance(period).await;
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, period)));
        assert!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .is_some()
        );
        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 7.0);
    }

    #[tokio::test(start_paused = true)]
    async fn missing_and_invalid_hints_are_no_ops() {
        let registry = Registry::new();
        let metrics = KvMetrics::new(&registry);
        let controller = BatchWriteFlowController::new("invalid".to_owned(), Some(metrics.clone()));
        set_effective_qps_fixture(&controller, 42.0);
        controller.on_server_feedback(Some(&rate_limit_info(1.0, DEFAULT_PERIOD)));
        let expected = observation_snapshot(&controller);

        let invalid_hints = [
            None,
            Some(rate_limit_info(0.0, DEFAULT_PERIOD)),
            Some(rate_limit_info(f64::NAN, DEFAULT_PERIOD)),
            Some(rate_limit_info(f64::INFINITY, DEFAULT_PERIOD)),
            Some(RateLimitInfo {
                period: None,
                factor: 1.0,
            }),
            Some(raw_rate_limit_info(1.0, -1, 0)),
            Some(raw_rate_limit_info(1.0, 0, -1)),
            Some(raw_rate_limit_info(1.0, 0, 1_000_000_000)),
            Some(raw_rate_limit_info(1.0, 0, 0)),
        ];
        for invalid_hint in invalid_hints {
            controller.on_server_feedback(invalid_hint.as_ref());
            assert_eq!(observation_snapshot(&controller), expected);
        }
        assert_eq!(
            server_feedback_count(&metrics, "invalid", "neutral", "queued"),
            1
        );
        for direction in ["decrease", "increase"] {
            for outcome in ["queued", "rejected"] {
                assert_eq!(
                    server_feedback_count(&metrics, "invalid", direction, outcome),
                    0
                );
            }
        }
        assert_eq!(
            server_feedback_count(&metrics, "invalid", "neutral", "rejected"),
            0
        );

        let nanos_only = BatchWriteFlowController::new("nanos".to_owned(), None);
        set_effective_qps_fixture(&nanos_only, 42.0);
        nanos_only.on_server_feedback(Some(&raw_rate_limit_info(1.0, 0, 1)));
        let state = nanos_only
            .state
            .lock()
            .expect("flow-control state mutex poisoned");
        assert_eq!(state.effective_qps, 42.0);
        assert_eq!(state.pending.server_period, Some(Duration::from_nanos(1)));
        assert!(state.observation.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn server_feedback_respects_period() {
        let registry = Registry::new();
        let metrics = KvMetrics::new(&registry);
        let controller = BatchWriteFlowController::new("period".to_owned(), Some(metrics.clone()));
        assert_eq!(
            metrics
                .kv_bt_flow_control_effective_qps
                .with_label_values(&["period"])
                .get(),
            INITIAL_QPS
        );
        assert_eq!(
            metrics
                .kv_bt_flow_control_last_observation_start_qps
                .with_label_values(&["period"])
                .get(),
            0.0
        );

        let period = Duration::from_secs(2);
        controller.on_server_feedback(Some(&rate_limit_info(1.0, period)));
        assert_eq!(
            server_feedback_count(&metrics, "period", "neutral", "queued"),
            1
        );
        assert_eq!(
            flow_control_event_count(&metrics, "period", "observation_started"),
            1
        );
        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 10.0);
        assert_eq!(
            flow_control_event_count(&metrics, "period", "observation_completed"),
            1
        );

        tokio::time::advance(Duration::from_secs(1)).await;
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, period)));
        fail_admission(&controller, Code::ResourceExhausted);
        assert_eq!(
            flow_control_event_count(&metrics, "period", "feedback_rejected"),
            2
        );
        assert_eq!(
            server_feedback_count(&metrics, "period", "decrease", "rejected"),
            1
        );
        assert_eq!(
            server_feedback_count(&metrics, "period", "decrease", "queued"),
            0
        );
        assert!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .is_none()
        );

        tokio::time::advance(Duration::from_secs(1)).await;
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, period)));
        assert_eq!(
            flow_control_event_count(&metrics, "period", "observation_started"),
            2
        );
        assert_effective_qps(&controller, 10.0);
        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 7.0);
        assert_eq!(
            flow_control_event_count(&metrics, "period", "observation_completed"),
            2
        );
        assert_eq!(
            server_feedback_count(&metrics, "period", "decrease", "queued"),
            1
        );
    }

    #[tokio::test(start_paused = true)]
    async fn latency_feedback_metrics_record_generated_directions() {
        let registry = Registry::new();
        let metrics = KvMetrics::new(&registry);
        let controller = BatchWriteFlowController::new("latency".to_owned(), Some(metrics.clone()));

        learn_healthy_baseline(&controller);
        assert_eq!(latency_feedback_count(&metrics, "latency", "increase"), 0);

        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        assert_eq!(latency_feedback_count(&metrics, "latency", "increase"), 1);

        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(40));
        assert_eq!(latency_feedback_count(&metrics, "latency", "reanchor"), 1);

        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(100));
        assert_eq!(latency_feedback_count(&metrics, "latency", "decrease"), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn only_qualifying_errors_queue_feedback() {
        for code in [
            Code::DeadlineExceeded,
            Code::Unavailable,
            Code::ResourceExhausted,
        ] {
            let controller = BatchWriteFlowController::new("error".to_owned(), None);
            fail_admission(&controller, code);
            {
                let state = controller
                    .state
                    .lock()
                    .expect("flow-control state mutex poisoned");
                assert_eq!(state.pending.server_factor, Some(MIN_FACTOR));
                assert_eq!(state.pending.server_period, Some(DEFAULT_PERIOD));
                assert_eq!(state.latency_samples, 0);
                assert!(state.observation.is_some());
            }
            admit_rpcs(&controller, 10).await;
            finish_observation(&controller).await;
            assert_effective_qps(&controller, 7.0);
        }

        let controller = BatchWriteFlowController::new("other-error".to_owned(), None);
        fail_admission(&controller, Code::NotFound);
        assert_effective_qps(&controller, INITIAL_QPS);
        assert!(
            controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned")
                .observation
                .is_none()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn spaces_permits_without_burst_credit() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        set_effective_qps_fixture(&controller, 2.0);
        tokio::time::advance(Duration::from_secs(10)).await;

        drop(controller.admit_rpc().await);
        let first_permit = Instant::now();
        drop(controller.admit_rpc().await);
        let second_permit = Instant::now();

        assert_eq!(
            second_permit.duration_since(first_permit),
            Duration::from_millis(500)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn queued_permits_are_rescheduled_after_decrease() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        drop(controller.admit_rpc().await);

        let first_queued = tokio::spawn({
            let controller = controller.clone();
            async move {
                drop(controller.admit_rpc().await);
                Instant::now()
            }
        });
        tokio::task::yield_now().await;
        let second_queued = tokio::spawn({
            let controller = controller.clone();
            async move {
                drop(controller.admit_rpc().await);
                Instant::now()
            }
        });
        tokio::task::yield_now().await;

        apply_ready_server_decrease(&controller);
        assert_effective_qps(&controller, INITIAL_QPS * MIN_FACTOR);

        tokio::time::advance(Duration::from_millis(200)).await;
        tokio::task::yield_now().await;
        assert_eq!(
            usize::from(first_queued.is_finished()) + usize::from(second_queued.is_finished()),
            1
        );

        tokio::time::advance(Duration::from_millis(200)).await;
        let first_start = first_queued.await.unwrap();
        let second_start = second_queued.await.unwrap();
        let spacing = if first_start < second_start {
            second_start.duration_since(first_start)
        } else {
            first_start.duration_since(second_start)
        };
        assert!(spacing >= Duration::from_secs_f64(1.0 / (INITIAL_QPS * MIN_FACTOR)));
    }

    #[tokio::test(start_paused = true)]
    async fn stale_rate_generation_latency_is_ignored() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        let stale_admission = controller.admit_rpc().await;
        apply_ready_server_decrease(&controller);
        make_latency_evaluation_ready(&controller);

        stale_admission.complete();
        assert_eq!(latency_samples(&controller), 0);
        assert_eq!(baseline_micros(&controller), None);

        admission_with_elapsed(&controller, Duration::from_millis(20)).complete();
        assert_eq!(latency_samples(&controller), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn severe_latency_reduces_observed_rate_limit() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(200));
        let expected_alpha = non_healthy_baseline_alpha(LATENCY_EVALUATION_PERIOD);
        let expected_baseline = 20_000.0 + expected_alpha * (200_000.0 - 20_000.0);
        let actual_baseline =
            baseline_micros(&controller).expect("baseline should remain available");
        assert!(
            (actual_baseline - expected_baseline).abs() < 1e-9,
            "expected slowly adapted baseline {expected_baseline}, got {actual_baseline}"
        );
        assert_eq!(
            pending_latency_feedback(&controller),
            Some(LatencyFeedback::Decrease)
        );
        assert_effective_qps(&controller, INITIAL_QPS);

        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 7.0);
    }

    #[tokio::test(start_paused = true)]
    async fn first_latency_window_establishes_baseline_without_authorizing_growth() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_secs(5));

        assert_eq!(baseline_micros(&controller), Some(5_000_000.0));
        assert_eq!(pending_latency_feedback(&controller), None);
        assert_effective_qps(&controller, INITIAL_QPS);
    }

    #[tokio::test(start_paused = true)]
    async fn elevated_latency_reanchors_to_observed_rate() {
        let controller = BatchWriteFlowController::new("local".to_owned(), None);
        learn_healthy_baseline(&controller);
        set_effective_qps_fixture(&controller, 100.0);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(40));
        assert_eq!(
            pending_latency_feedback(&controller),
            Some(LatencyFeedback::Reanchor)
        );
        assert!(baseline_micros(&controller).is_some_and(|baseline| baseline > 20_000.0));
        admit_rpcs(&controller, 20).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 20.0);

        let with_server = BatchWriteFlowController::new("server".to_owned(), None);
        learn_healthy_baseline(&with_server);
        set_effective_qps_fixture(&with_server, 100.0);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        with_server.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, DEFAULT_PERIOD)));
        record_latency_samples(&with_server, MIN_WINDOW_SAMPLES, Duration::from_millis(40));
        admit_rpcs(&with_server, 20).await;
        finish_observation(&with_server).await;
        assert_effective_qps(&with_server, 14.0);
        assert!(baseline_micros(&with_server).is_some_and(|baseline| baseline > 20_000.0));
    }

    #[tokio::test(start_paused = true)]
    async fn healthy_latency_uses_local_recovery_without_capping_server_growth() {
        let local = BatchWriteFlowController::new("local".to_owned(), None);
        learn_healthy_baseline(&local);
        set_effective_qps_fixture(&local, 100.0);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&local, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        assert_eq!(
            pending_latency_feedback(&local),
            Some(LatencyFeedback::Increase)
        );
        admit_rpcs(&local, 90).await;
        finish_observation(&local).await;
        assert_effective_qps(&local, 100.0 * HEALTHY_RECOVERY_FACTOR);

        let with_server = BatchWriteFlowController::new("server".to_owned(), None);
        learn_healthy_baseline(&with_server);
        set_effective_qps_fixture(&with_server, 100.0);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&with_server, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        with_server.on_server_feedback(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));
        admit_rpcs(&with_server, 90).await;
        finish_observation(&with_server).await;
        assert_effective_qps(&with_server, 100.0 * MAX_FACTOR);
    }

    #[tokio::test(start_paused = true)]
    async fn sparse_latency_samples_accumulate_until_floor() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;

        record_latency_samples(&controller, 3, Duration::from_millis(200));
        assert_eq!(latency_samples(&controller), 3);
        assert_eq!(pending_latency_feedback(&controller), None);

        record_latency_samples(&controller, 2, Duration::from_millis(200));
        assert_eq!(latency_samples(&controller), 0);
        assert_eq!(
            pending_latency_feedback(&controller),
            Some(LatencyFeedback::Decrease)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn healthy_latency_updates_baseline_with_ewma() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        make_latency_evaluation_ready(&controller);

        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(25));

        assert_eq!(baseline_micros(&controller), Some(21_000.0));
    }

    #[tokio::test(start_paused = true)]
    async fn ready_elevated_window_receives_minimum_evidence_credit() {
        const ELEVATED_LATENCY: Duration = Duration::from_millis(40);

        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, ELEVATED_LATENCY);

        let expected_alpha = non_healthy_baseline_alpha(LATENCY_EVALUATION_PERIOD);
        let expected_baseline = 20_000.0 + expected_alpha * (40_000.0 - 20_000.0);
        let actual_baseline =
            baseline_micros(&controller).expect("baseline should remain available");
        assert!(
            (actual_baseline - expected_baseline).abs() < 1e-9,
            "expected minimum evidence credit baseline {expected_baseline}, got {actual_baseline}"
        );
        assert_eq!(
            pending_latency_feedback(&controller),
            Some(LatencyFeedback::Reanchor)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn one_elevated_window_does_not_authorize_growth() {
        const ELEVATED_LATENCY: Duration = Duration::from_millis(40);

        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        record_latency_window_over(&controller, ELEVATED_LATENCY, LATENCY_EVALUATION_PERIOD).await;

        assert!(baseline_micros(&controller).is_some_and(|baseline| {
            baseline > 20_000.0
                && baseline < ELEVATED_LATENCY.as_micros() as f64 / ELEVATED_LATENCY_RATIO
        }));
        assert_eq!(
            pending_latency_feedback(&controller),
            Some(LatencyFeedback::Reanchor)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn minimum_qps_caps_elevated_baseline_evidence_to_evaluation_period() {
        const ELEVATED_LATENCY: Duration = Duration::from_millis(40);
        const EVIDENCE_DURATION: Duration = Duration::from_secs(50);

        let frequent = BatchWriteFlowController::new("frequent".to_owned(), None);
        learn_healthy_baseline(&frequent);
        record_latency_window_over(&frequent, ELEVATED_LATENCY, LATENCY_EVALUATION_PERIOD).await;

        let sparse = BatchWriteFlowController::new("sparse".to_owned(), None);
        learn_healthy_baseline(&sparse);
        set_effective_qps_fixture(&sparse, MIN_QPS);
        record_latency_window_over(&sparse, ELEVATED_LATENCY, EVIDENCE_DURATION).await;

        let frequent_baseline =
            baseline_micros(&frequent).expect("frequent baseline should remain available");
        let sparse_baseline =
            baseline_micros(&sparse).expect("sparse baseline should remain available");
        assert!(
            (frequent_baseline - sparse_baseline).abs() < 1e-9,
            "expected minimum-QPS evidence cap to match one evaluation period, got frequent={frequent_baseline} sparse={sparse_baseline}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn low_qps_baseline_evidence_matches_frequent_evaluations() {
        const SYNTHETIC_LOW_QPS: f64 = 0.1;
        const ELEVATED_LATENCY: Duration = Duration::from_millis(40);
        const SAMPLE_COLLECTION_TIME: Duration = Duration::from_secs(50);
        const EVALUATION_WINDOWS: u32 = 5;

        let frequent = BatchWriteFlowController::new("frequent".to_owned(), None);
        learn_healthy_baseline(&frequent);
        for _ in 0..EVALUATION_WINDOWS {
            record_latency_window_over(&frequent, ELEVATED_LATENCY, LATENCY_EVALUATION_PERIOD)
                .await;
        }

        let low_qps = BatchWriteFlowController::new("low-qps".to_owned(), None);
        learn_healthy_baseline(&low_qps);
        // Exercise the rate-aware branch independently of the configured admission floor.
        set_effective_qps_fixture(&low_qps, SYNTHETIC_LOW_QPS);
        record_latency_window_over(&low_qps, ELEVATED_LATENCY, SAMPLE_COLLECTION_TIME).await;

        let frequent_baseline =
            baseline_micros(&frequent).expect("frequent baseline should remain available");
        let low_qps_baseline =
            baseline_micros(&low_qps).expect("low-QPS baseline should remain available");
        assert!(
            (frequent_baseline - low_qps_baseline).abs() < 1e-9,
            "expected one low-QPS sample window to receive the same evidence as {EVALUATION_WINDOWS} frequent windows, got frequent={frequent_baseline} low_qps={low_qps_baseline}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn idle_gap_cannot_accelerate_elevated_baseline() {
        const ELEVATED_LATENCY: Duration = Duration::from_millis(40);
        const IDLE_GAP: Duration = Duration::from_secs(24 * 60 * 60);

        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        set_effective_qps_fixture(&controller, MIN_QPS);
        tokio::time::advance(IDLE_GAP).await;
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, ELEVATED_LATENCY);

        let expected_alpha = non_healthy_baseline_alpha(LATENCY_EVALUATION_PERIOD);
        let expected_baseline = 20_000.0 + expected_alpha * (40_000.0 - 20_000.0);
        let actual_baseline =
            baseline_micros(&controller).expect("baseline should remain available");
        assert!(
            (actual_baseline - expected_baseline).abs() < 1e-9,
            "expected evidence-capped baseline {expected_baseline}, got {actual_baseline}"
        );
        assert_eq!(
            pending_latency_feedback(&controller),
            Some(LatencyFeedback::Reanchor)
        );
    }
}
