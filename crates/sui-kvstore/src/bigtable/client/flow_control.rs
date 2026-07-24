// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Bigtable batch-write flow control starts configured clients at a conservative `MutateRows`
//! admission rate, then derives later limits from observed RPC starts. Server decreases,
//! qualifying RPC errors, and write latency contribute feedback to complete observation windows.
//! Rate increases require both an absolute-safe latency and a healthy relative baseline.

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
const MIN_QPS: f64 = 0.1;
const MAX_QPS: f64 = 100_000.0;
const MIN_FACTOR: f64 = 0.7;
const MAX_FACTOR: f64 = 1.3;
const HEALTHY_RECOVERY_FACTOR: f64 = 1.05;
const UPWARD_UTILIZATION_THRESHOLD: f64 = 0.8;
const ELEVATED_LATENCY_RATIO: f64 = 1.5;
// Never learn a multi-second startup as healthy. One second is the maximum completion time this
// controller treats as safe while probing upward.
const MAX_HEALTHY_WRITE_LATENCY_MICROS: f64 = 1_000_000.0;
const SEVERE_LATENCY_RATIO: f64 = 3.0;
const BASELINE_EWMA_ALPHA: f64 = 0.2;
const BASELINE_STALE_EVALS: u32 = 90;
const BASELINE_STALE_ALPHA: f64 = 0.05;
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
    if window_avg_micros > MAX_HEALTHY_WRITE_LATENCY_MICROS
        || baseline_micros
            .is_some_and(|baseline| window_avg_micros > SEVERE_LATENCY_RATIO * baseline)
    {
        WriteLatencyCondition::Severe
    } else if baseline_micros
        .is_some_and(|baseline| window_avg_micros > ELEVATED_LATENCY_RATIO * baseline)
    {
        WriteLatencyCondition::Elevated
    } else {
        WriteLatencyCondition::Healthy
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
    baseline_write_latency_micros: Option<f64>,
    non_healthy_evals: u32,
}

impl ControllerState {
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

    fn reserve_permit(&mut self, now: Instant) -> PermitReservation {
        let permit_interval = Duration::from_secs_f64(1.0 / self.effective_qps);
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

struct RateUpdate {
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
                baseline_write_latency_micros: None,
                non_healthy_evals: 0,
            }),
            client_name,
            metrics,
        });

        if let Some(metrics) = &controller.metrics {
            metrics
                .kv_bt_flow_control_limited
                .with_label_values(&[&controller.client_name])
                .set(1);
            metrics
                .kv_bt_flow_control_effective_qps
                .with_label_values(&[&controller.client_name])
                .set(INITIAL_QPS);
            metrics
                .kv_bt_flow_control_observed_start_qps
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
            let (rate_update, reservation) = {
                let mut state = self
                    .state
                    .lock()
                    .expect("flow-control state mutex poisoned");
                let now = Instant::now();
                let rate_update = Self::finish_observation_if_ready(&mut state, now);
                let reservation = state.reserve_permit(now);
                (rate_update, reservation)
            };
            if let Some(rate_update) = rate_update {
                self.emit_rate_update_telemetry(rate_update);
            }
            total_wait = total_wait.saturating_add(reservation.wait);
            if !reservation.wait.is_zero() {
                tokio::time::sleep(reservation.wait).await;
            }

            let (rate_update, admitted) = {
                let mut state = self
                    .state
                    .lock()
                    .expect("flow-control state mutex poisoned");
                let now = Instant::now();
                let rate_update = Self::finish_observation_if_ready(&mut state, now);
                let admitted = state.complete_reservation(reservation);
                (rate_update, admitted)
            };
            if let Some(rate_update) = rate_update {
                self.emit_rate_update_telemetry(rate_update);
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
    ) -> Option<RateUpdate> {
        let observation = state.observation.as_ref()?;
        let elapsed = now.saturating_duration_since(observation.started_at);
        if elapsed < OBSERVATION_WINDOW {
            return None;
        }
        if observation.rpc_starts == 0 {
            state.restart_or_close_empty_observation(now);
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
        // A paced one-second window can omit a boundary start. It proves utilization, but growth
        // compounds from the current limit rather than the sampled start rate.
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
            state.next_latency_evaluation_at = now + LATENCY_EVALUATION_PERIOD;
            state.next_permit_at = now;
        }
        if state.pending.server_factor.is_some() {
            state.next_server_update_at =
                now + state.pending.server_period.unwrap_or(DEFAULT_PERIOD);
        }
        state.observation = None;
        state.pending = PendingFeedback::default();

        Some(RateUpdate {
            observed_start_qps,
            effective_qps,
        })
    }

    fn emit_rate_update_telemetry(&self, rate_update: RateUpdate) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_effective_qps
                .with_label_values(&[&self.client_name])
                .set(rate_update.effective_qps);
            metrics
                .kv_bt_flow_control_observed_start_qps
                .with_label_values(&[&self.client_name])
                .set(rate_update.observed_start_qps);
        }
        self.increment_rate_update("applied");
        info!(
            observed_start_qps = rate_update.observed_start_qps,
            effective_qps = rate_update.effective_qps,
            "Batch write flow control: effective rate updated"
        );
    }

    fn on_server_feedback(&self, info: Option<&RateLimitInfo>) {
        let Some((factor, period)) = Self::validated_server_feedback(info) else {
            return;
        };
        let (rate_update, started_observation, server_feedback_rejected) = {
            let mut state = self
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            let now = Instant::now();
            let rate_update = Self::finish_observation_if_ready(&mut state, now);
            let server_feedback_allowed = now >= state.next_server_update_at;
            let healthy_growth_pending = state.pending.allows_growth();
            let (started_observation, server_feedback_rejected) =
                if server_feedback_allowed && (factor <= 1.0 || healthy_growth_pending) {
                    (
                        Self::queue_server_feedback(&mut state, now, factor, period),
                        false,
                    )
                } else {
                    (false, true)
                };
            (rate_update, started_observation, server_feedback_rejected)
        };
        if let Some(rate_update) = rate_update {
            self.emit_rate_update_telemetry(rate_update);
        }
        if started_observation {
            self.emit_observation_started_telemetry();
        }
        if server_feedback_rejected {
            self.emit_server_feedback_rejected_telemetry();
        }
    }

    fn complete_error(&self, code: Code) {
        if !is_overload_error(code) {
            return;
        }

        let (rate_update, started_observation, server_feedback_rejected) = {
            let mut state = self
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            let now = Instant::now();
            let rate_update = Self::finish_observation_if_ready(&mut state, now);
            let server_feedback_allowed = now >= state.next_server_update_at;
            let (started_observation, server_feedback_rejected) = if server_feedback_allowed {
                (
                    Self::queue_server_feedback(&mut state, now, MIN_FACTOR, DEFAULT_PERIOD),
                    false,
                )
            } else {
                (false, true)
            };
            (rate_update, started_observation, server_feedback_rejected)
        };
        if let Some(rate_update) = rate_update {
            self.emit_rate_update_telemetry(rate_update);
        }
        if started_observation {
            self.emit_observation_started_telemetry();
        }
        if server_feedback_rejected {
            self.emit_server_feedback_rejected_telemetry();
        }
    }

    fn validated_server_feedback(info: Option<&RateLimitInfo>) -> Option<(f64, Duration)> {
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
        Some((info.factor.clamp(MIN_FACTOR, MAX_FACTOR), period))
    }

    fn queue_server_feedback(
        state: &mut ControllerState,
        now: Instant,
        factor: f64,
        period: Duration,
    ) -> bool {
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
        self.increment_rate_update("pending");
        info!("Batch write flow control: feedback observation started");
    }

    fn emit_server_feedback_rejected_telemetry(&self) {
        self.increment_rate_update("rejected");
    }

    fn increment_rate_update(&self, kind: &str) {
        if let Some(metrics) = &self.metrics {
            metrics
                .kv_bt_flow_control_rate_updates
                .with_label_values(&[self.client_name.as_str(), kind])
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
        state.latency_total_micros = 0;
        state.latency_samples = 0;
        state.next_latency_evaluation_at = now + LATENCY_EVALUATION_PERIOD;

        let condition =
            classify_write_latency(window_avg_micros, state.baseline_write_latency_micros);
        let latency_feedback = match condition {
            WriteLatencyCondition::Severe => {
                state.non_healthy_evals = state.non_healthy_evals.saturating_add(1);
                Some(LatencyFeedback::Decrease)
            }
            WriteLatencyCondition::Elevated => {
                state.non_healthy_evals = state.non_healthy_evals.saturating_add(1);
                if state.non_healthy_evals >= BASELINE_STALE_EVALS
                    && let Some(baseline) = state.baseline_write_latency_micros.as_mut()
                {
                    *baseline += BASELINE_STALE_ALPHA * (window_avg_micros - *baseline);
                }
                Some(LatencyFeedback::Reanchor)
            }
            WriteLatencyCondition::Healthy => {
                state.non_healthy_evals = 0;
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
            started_observation,
        })
    }

    fn complete_stream(&self, rate_generation: u64, elapsed: Duration) {
        let (rate_update, latency_evaluation) = {
            let mut state = self
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            let now = Instant::now();
            let rate_update = Self::finish_observation_if_ready(&mut state, now);
            let latency_evaluation = if rate_generation != state.rate_generation {
                None
            } else {
                let elapsed_micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
                state.latency_total_micros =
                    state.latency_total_micros.saturating_add(elapsed_micros);
                state.latency_samples = state.latency_samples.saturating_add(1);
                Self::evaluate_latency_if_ready(&mut state, now)
            };
            (rate_update, latency_evaluation)
        };

        if let Some(rate_update) = rate_update {
            self.emit_rate_update_telemetry(rate_update);
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
                    .kv_bt_flow_control_write_latency_ms
                    .with_label_values(&[&self.client_name])
                    .set(latency_evaluation.window_avg_micros / 1_000.0);
                if let Some(baseline_micros) = latency_evaluation.baseline_micros {
                    metrics
                        .kv_bt_flow_control_write_latency_baseline_ms
                        .with_label_values(&[&self.client_name])
                        .set(baseline_micros / 1_000.0);
                }
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
        let rate_update = {
            let mut state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            BatchWriteFlowController::finish_observation_if_ready(&mut state, Instant::now())
        };
        if let Some(rate_update) = rate_update {
            controller.emit_rate_update_telemetry(rate_update);
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

    fn rate_update_count(metrics: &KvMetrics, client: &str, kind: &str) -> u64 {
        metrics
            .kv_bt_flow_control_rate_updates
            .with_label_values(&[client, kind])
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
        drop(lower.admit_rpc().await);
        tokio::time::advance(Duration::from_secs(10)).await;
        finish_ready_observation(&lower);
        assert_effective_qps(&lower, MIN_QPS);

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
    async fn healthy_growth_feedback_expires_after_empty_window() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);
        tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(20));
        controller.on_server_feedback(Some(&rate_limit_info(MAX_FACTOR, DEFAULT_PERIOD)));

        tokio::time::advance(OBSERVATION_WINDOW).await;
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
        let controller = BatchWriteFlowController::new("invalid".to_owned(), None);
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
                .kv_bt_flow_control_limited
                .with_label_values(&["period"])
                .get(),
            1
        );
        assert_eq!(
            metrics
                .kv_bt_flow_control_effective_qps
                .with_label_values(&["period"])
                .get(),
            INITIAL_QPS
        );
        assert_eq!(
            metrics
                .kv_bt_flow_control_observed_start_qps
                .with_label_values(&["period"])
                .get(),
            0.0
        );

        let period = Duration::from_secs(2);
        controller.on_server_feedback(Some(&rate_limit_info(1.0, period)));
        assert_eq!(rate_update_count(&metrics, "period", "pending"), 1);
        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 10.0);
        assert_eq!(rate_update_count(&metrics, "period", "applied"), 1);

        tokio::time::advance(Duration::from_secs(1)).await;
        controller.on_server_feedback(Some(&rate_limit_info(MIN_FACTOR, period)));
        fail_admission(&controller, Code::ResourceExhausted);
        assert_eq!(rate_update_count(&metrics, "period", "rejected"), 2);
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
        assert_eq!(rate_update_count(&metrics, "period", "pending"), 2);
        assert_effective_qps(&controller, 10.0);
        admit_rpcs(&controller, 10).await;
        finish_observation(&controller).await;
        assert_effective_qps(&controller, 7.0);
        assert_eq!(rate_update_count(&metrics, "period", "applied"), 2);
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
    async fn unsafe_absolute_latency_never_becomes_baseline_or_authorizes_growth() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        make_latency_evaluation_ready(&controller);
        record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_secs(5));

        assert_eq!(baseline_micros(&controller), None);
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
        assert_eq!(baseline_micros(&controller), Some(20_000.0));
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
        assert_eq!(baseline_micros(&with_server), Some(20_000.0));
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
    async fn stale_elevated_baseline_drifts_until_healthy() {
        let controller = BatchWriteFlowController::new("test".to_owned(), None);
        learn_healthy_baseline(&controller);

        let mut reached_healthy = false;
        for _ in 0..BASELINE_STALE_EVALS + 10 {
            tokio::time::advance(LATENCY_EVALUATION_PERIOD).await;
            record_latency_samples(&controller, MIN_WINDOW_SAMPLES, Duration::from_millis(40));
            let state = controller
                .state
                .lock()
                .expect("flow-control state mutex poisoned");
            if state.non_healthy_evals == 0 {
                reached_healthy = true;
                break;
            }
        }

        assert!(
            reached_healthy,
            "stale elevated baseline never reached healthy"
        );
        assert!(baseline_micros(&controller).is_some_and(|baseline| baseline > 20_000.0));
    }
}
