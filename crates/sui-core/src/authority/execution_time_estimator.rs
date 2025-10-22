// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    sync::{Arc, Weak},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use super::authority_per_epoch_store::AuthorityPerEpochStore;
use super::weighted_moving_average::WeightedMovingAverage;
use crate::consensus_adapter::SubmitToConsensus;
use governor::{Quota, RateLimiter, clock::MonotonicClock};
use itertools::Itertools;
use lru::LruCache;
#[cfg(not(msim))]
use mysten_common::in_antithesis;
use mysten_common::{assert_reachable, debug_fatal, in_test_configuration};
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use rand::{Rng, SeedableRng, random, rngs, thread_rng};
use simple_moving_average::{SMA, SingleSumSMA};
use sui_config::node::ExecutionTimeObserverConfig;
use sui_protocol_config::{ExecutionTimeEstimateParams, PerObjectCongestionControlMode};
use sui_types::{
    base_types::ObjectID,
    committee::Committee,
    error::SuiErrorKind,
    execution::{ExecutionTimeObservationKey, ExecutionTiming},
    messages_consensus::{AuthorityIndex, ConsensusTransaction, ExecutionTimeObservation},
    transaction::{
        Command, ProgrammableTransaction, StoredExecutionTimeObservations, TransactionData,
        TransactionDataAPI, TransactionKind,
    },
};
use tokio::{sync::mpsc, time::Instant};
use tracing::{debug, info, trace, warn};

// TODO: Move this into ExecutionTimeObserverConfig, if we switch to a moving average
// implmentation without the window size in the type.
const SMA_LOCAL_OBSERVATION_WINDOW_SIZE: usize = 20;
const OBJECT_UTILIZATION_METRIC_HASH_MODULUS: u8 = 32;

/// Determines whether to inject synthetic execution time in Antithesis environments.
///
/// This function checks two conditions:
/// 1. Whether the code is running in an Antithesis environment
/// 2. Whether injection is enabled via the `ANTITHESIS_ENABLE_EXECUTION_TIME_INJECTION` env var
///    (enabled by default)
#[cfg(not(msim))]
fn antithesis_enable_injecting_synthetic_execution_time() -> bool {
    use std::sync::OnceLock;
    static ENABLE_INJECTION: OnceLock<bool> = OnceLock::new();
    *ENABLE_INJECTION.get_or_init(|| {
        if !in_antithesis() {
            return false;
        }

        std::env::var("ANTITHESIS_ENABLE_EXECUTION_TIME_INJECTION")
            .map(|v| v.to_lowercase() == "true" || v == "1")
            .unwrap_or(true)
    })
}

// Collects local execution time estimates to share via consensus.
pub struct ExecutionTimeObserver {
    epoch_store: Weak<AuthorityPerEpochStore>,
    consensus_adapter: Box<dyn SubmitToConsensus>,

    protocol_params: ExecutionTimeEstimateParams,
    config: ExecutionTimeObserverConfig,

    local_observations: LruCache<ExecutionTimeObservationKey, LocalObservations>,

    // For each object, tracks the amount of time above our utilization target that we spent
    // executing transactions. This is used to decide which observations should be shared
    // via consensus.
    object_utilization_tracker: LruCache<ObjectID, ObjectUtilization>,

    // Sorted list of recently indebted objects, updated by consensus handler.
    indebted_objects: Vec<ObjectID>,

    sharing_rate_limiter: RateLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::MonotonicClock,
        governor::middleware::NoOpMiddleware<
            <governor::clock::MonotonicClock as governor::clock::Clock>::Instant,
        >,
    >,

    next_generation_number: u64,
}

#[derive(Debug, Clone)]
pub struct LocalObservations {
    moving_average: SingleSumSMA<Duration, u32, SMA_LOCAL_OBSERVATION_WINDOW_SIZE>,
    weighted_moving_average: WeightedMovingAverage,
    last_shared: Option<(Duration, Instant)>,
    config: ExecutionTimeObserverConfig,
}

impl LocalObservations {
    fn new(config: ExecutionTimeObserverConfig, default_duration: Duration) -> Self {
        let window_size = config.weighted_moving_average_window_size();
        Self {
            moving_average: SingleSumSMA::from_zero(Duration::ZERO),
            weighted_moving_average: WeightedMovingAverage::new(
                default_duration.as_micros() as u64,
                window_size,
            ),
            last_shared: None,
            config,
        }
    }

    fn add_sample(&mut self, duration: Duration, gas_price: u64) {
        self.moving_average.add_sample(duration);
        self.weighted_moving_average
            .add_sample(duration.as_micros() as u64, gas_price);
    }

    fn get_average(&self) -> Duration {
        if self.config.enable_gas_price_weighting() {
            Duration::from_micros(self.weighted_moving_average.get_weighted_average())
        } else {
            self.moving_average.get_average()
        }
    }

    fn diff_exceeds_threshold(
        &self,
        new_average: Duration,
        threshold: f64,
        min_interval: Duration,
    ) -> bool {
        let Some((last_shared, last_shared_timestamp)) = self.last_shared else {
            // Diff threshold exceeded by default if we haven't shared anything yet.
            return true;
        };

        if last_shared_timestamp.elapsed() < min_interval {
            return false;
        }

        if threshold >= 0.0 {
            // Positive threshold requires upward change.
            new_average
                .checked_sub(last_shared)
                .is_some_and(|diff| diff > last_shared.mul_f64(threshold))
        } else {
            // Negative threshold requires downward change.
            last_shared
                .checked_sub(new_average)
                .is_some_and(|diff| diff > last_shared.mul_f64(-threshold))
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectUtilization {
    excess_execution_time: Duration,
    last_measured: Option<Instant>,
    was_overutilized: bool, // true if the object has ever had excess_execution_time
}

impl ObjectUtilization {
    pub fn overutilized(&self, config: &ExecutionTimeObserverConfig) -> bool {
        self.excess_execution_time > config.observation_sharing_object_utilization_threshold()
    }
}

// Tracks local execution time observations and shares them via consensus.
impl ExecutionTimeObserver {
    pub fn spawn(
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_adapter: Box<dyn SubmitToConsensus>,
        config: ExecutionTimeObserverConfig,
    ) {
        let PerObjectCongestionControlMode::ExecutionTimeEstimate(protocol_params) = epoch_store
            .protocol_config()
            .per_object_congestion_control_mode()
        else {
            info!(
                "ExecutionTimeObserver disabled because per-object congestion control mode is not ExecutionTimeEstimate"
            );
            return;
        };

        let (tx_local_execution_time, mut rx_local_execution_time) =
            mpsc::channel(config.observation_channel_capacity().into());
        let (tx_object_debts, mut rx_object_debts) =
            mpsc::channel(config.object_debt_channel_capacity().into());
        epoch_store.set_local_execution_time_channels(tx_local_execution_time, tx_object_debts);

        // TODO: pre-populate local observations with stored data from prior epoch.
        let mut observer = Self {
            epoch_store: Arc::downgrade(&epoch_store),
            consensus_adapter,
            local_observations: LruCache::new(config.observation_cache_size()),
            object_utilization_tracker: LruCache::new(config.object_utilization_cache_size()),
            indebted_objects: Vec::new(),
            sharing_rate_limiter: RateLimiter::direct_with_clock(
                Quota::per_second(config.observation_sharing_rate_limit())
                    .allow_burst(config.observation_sharing_burst_limit()),
                &MonotonicClock,
            ),
            protocol_params,
            config,
            next_generation_number: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Sui did not exist prior to 1970")
                .as_micros()
                .try_into()
                .expect("This build of sui is not supported in the year 500,000"),
        };
        spawn_monitored_task!(epoch_store.within_alive_epoch(async move {
            loop {
                tokio::select! {
                    // TODO: add metrics for messages received.
                    Some(object_debts) = rx_object_debts.recv() => {
                        observer.update_indebted_objects(object_debts);
                    }
                    Some((tx, timings, total_duration, gas_price)) = rx_local_execution_time.recv() => {
                        observer
                            .record_local_observations(&tx, &timings, total_duration, gas_price);
                    }
                    else => { break }
                }
            }
            info!("shutting down ExecutionTimeObserver");
        }));
    }

    #[cfg(test)]
    fn new_for_testing(
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_adapter: Box<dyn SubmitToConsensus>,
        observation_sharing_object_utilization_threshold: Duration,
        enable_gas_price_weighting: bool,
    ) -> Self {
        let PerObjectCongestionControlMode::ExecutionTimeEstimate(protocol_params) = epoch_store
            .protocol_config()
            .per_object_congestion_control_mode()
        else {
            panic!(
                "tried to construct test ExecutionTimeObserver when congestion control mode is not ExecutionTimeEstimate"
            );
        };
        Self {
            epoch_store: Arc::downgrade(&epoch_store),
            consensus_adapter,
            protocol_params,
            config: ExecutionTimeObserverConfig {
                observation_sharing_object_utilization_threshold: Some(
                    observation_sharing_object_utilization_threshold,
                ),
                enable_gas_price_weighting: Some(enable_gas_price_weighting),
                ..ExecutionTimeObserverConfig::default()
            },
            local_observations: LruCache::new(NonZeroUsize::new(10000).unwrap()),
            object_utilization_tracker: LruCache::new(NonZeroUsize::new(50000).unwrap()),
            indebted_objects: Vec::new(),
            sharing_rate_limiter: RateLimiter::direct_with_clock(
                Quota::per_hour(std::num::NonZeroU32::MAX),
                &MonotonicClock,
            ),
            next_generation_number: SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Sui did not exist prior to 1970")
                .as_micros()
                .try_into()
                .expect("This build of sui is not supported in the year 500,000"),
        }
    }

    // Used by execution to report observed per-entry-point execution times to the estimator.
    // Updates moving averages and submits observation to consensus if local observation differs
    // from consensus median.
    // TODO: Consider more detailed heuristic to account for overhead outside of commands.
    fn record_local_observations(
        &mut self,
        tx: &ProgrammableTransaction,
        timings: &[ExecutionTiming],
        total_duration: Duration,
        gas_price: u64,
    ) {
        let _scope = monitored_scope("ExecutionTimeObserver::record_local_observations");

        // Simulate timing in test contexts to trigger congestion control.
        #[cfg(msim)]
        let should_inject = self.config.inject_synthetic_execution_time();
        #[cfg(not(msim))]
        let should_inject = antithesis_enable_injecting_synthetic_execution_time();

        if should_inject {
            let (generated_timings, generated_duration) = self.generate_test_timings(tx, timings);
            self.record_local_observations_timing(
                tx,
                &generated_timings,
                generated_duration,
                gas_price,
            )
        } else {
            self.record_local_observations_timing(tx, timings, total_duration, gas_price)
        }
    }

    fn record_local_observations_timing(
        &mut self,
        tx: &ProgrammableTransaction,
        timings: &[ExecutionTiming],
        total_duration: Duration,
        gas_price: u64,
    ) {
        assert!(tx.commands.len() >= timings.len());

        let Some(epoch_store) = self.epoch_store.upgrade() else {
            debug!("epoch is ending, dropping execution time observation");
            return;
        };

        let mut uses_indebted_object = false;

        // Update the accumulated excess execution time for each mutable shared object
        // used in this transaction, and determine the max overage.
        let max_excess_per_object_execution_time = tx
            .shared_input_objects()
            .filter_map(|obj| obj.mutability.is_mutable().then_some(obj.id))
            .map(|id| {
                // Mark if any object used in the tx is indebted.
                if !uses_indebted_object && self.indebted_objects.binary_search(&id).is_ok() {
                    uses_indebted_object = true;
                }

                // For each object:
                // - add the execution time of the current transaction to the tracker
                // - subtract the maximum amount of time available for execution according
                //   to our utilization target since the last report was received
                //   (clamping to zero)
                //
                // What remains is the amount of excess time spent executing transactions on
                // the object above the intended limit. If this value is greater than zero,
                // it means the object is overutilized.
                let now = Instant::now();
                let utilization =
                    self.object_utilization_tracker
                        .get_or_insert_mut(id, || ObjectUtilization {
                            excess_execution_time: Duration::ZERO,
                            last_measured: None,
                            was_overutilized: false,
                        });
                let overutilized_at_start = utilization.overutilized(&self.config);
                utilization.excess_execution_time += total_duration;
                utilization.excess_execution_time =
                    utilization.excess_execution_time.saturating_sub(
                        utilization
                            .last_measured
                            .map(|last_measured| {
                                now.duration_since(last_measured)
                                    .mul_f64(self.protocol_params.target_utilization as f64 / 100.0)
                            })
                            .unwrap_or(Duration::MAX),
                    );
                utilization.last_measured = Some(now);
                if utilization.overutilized(&self.config) {
                    utilization.was_overutilized = true;
                }

                // Update overutilized objects metrics.
                if !overutilized_at_start && utilization.overutilized(&self.config) {
                    trace!("object {id:?} is overutilized");
                    epoch_store
                        .metrics
                        .epoch_execution_time_observer_overutilized_objects
                        .inc();
                } else if overutilized_at_start && !utilization.overutilized(&self.config) {
                    epoch_store
                        .metrics
                        .epoch_execution_time_observer_overutilized_objects
                        .dec();
                }
                if utilization.was_overutilized {
                    let key = if self.config.report_object_utilization_metric_with_full_id() {
                        id.to_string()
                    } else {
                        let key_lsb = id.into_bytes()[ObjectID::LENGTH - 1];
                        let hash = key_lsb % OBJECT_UTILIZATION_METRIC_HASH_MODULUS;
                        format!("{:x}", hash)
                    };

                    epoch_store
                        .metrics
                        .epoch_execution_time_observer_object_utilization
                        .with_label_values(&[key.as_str()])
                        .inc_by(total_duration.as_secs_f64());
                }

                utilization.excess_execution_time
            })
            .max()
            .unwrap_or(Duration::ZERO);
        epoch_store
            .metrics
            .epoch_execution_time_observer_utilization_cache_size
            .set(self.object_utilization_tracker.len() as i64);

        let total_command_duration: Duration = timings.iter().map(|t| t.duration()).sum();
        let extra_overhead = total_duration - total_command_duration;

        let mut to_share = Vec::with_capacity(tx.commands.len());
        for (i, timing) in timings.iter().enumerate() {
            let command = &tx.commands[i];

            // Special-case handling for Publish command: only use hard-coded default estimate.
            if matches!(command, Command::Publish(_, _)) {
                continue;
            }

            // TODO: Consider using failure/success information in computing estimates.
            let mut command_duration = timing.duration();

            // Distribute overhead proportionally to each command's measured duration.
            let overhead_factor = if total_command_duration > Duration::ZERO {
                command_duration.as_secs_f64() / total_command_duration.as_secs_f64()
            } else {
                // divisor here must be >0 or this loop would not be running at all
                1.0 / (tx.commands.len() as f64)
            };
            command_duration += extra_overhead.mul_f64(overhead_factor);

            // For native commands, adjust duration by length of command's inputs/outputs.
            // This is sort of arbitrary, but hopefully works okay as a heuristic.
            command_duration = command_duration.div_f64(command_length(command).get() as f64);

            // Update gas-weighted moving-average observation for the command.
            let key = ExecutionTimeObservationKey::from_command(command);
            let local_observation = self.local_observations.get_or_insert_mut(key.clone(), || {
                LocalObservations::new(self.config.clone(), Duration::ZERO)
            });
            local_observation.add_sample(command_duration, gas_price);

            // Send a new observation through consensus if:
            // - our current moving average differs too much from the last one we shared, and
            // - the tx has at least one mutable shared object with utilization that's too high
            // TODO: Consider only sharing observations that disagree with consensus estimate.
            let new_average = local_observation.get_average();
            let mut should_share = false;

            // Share upward adjustments if an object is overutilized.
            if max_excess_per_object_execution_time
                >= self
                    .config
                    .observation_sharing_object_utilization_threshold()
                && local_observation.diff_exceeds_threshold(
                    new_average,
                    self.config.observation_sharing_diff_threshold(),
                    self.config.observation_sharing_min_interval(),
                )
            {
                should_share = true;
                epoch_store
                    .metrics
                    .epoch_execution_time_observations_sharing_reason
                    .with_label_values(&["utilization"])
                    .inc();
            };

            // Share downward adjustments if an object is indebted.
            if uses_indebted_object
                && local_observation.diff_exceeds_threshold(
                    new_average,
                    -self.config.observation_sharing_diff_threshold(),
                    self.config.observation_sharing_min_interval(),
                )
            {
                should_share = true;
                epoch_store
                    .metrics
                    .epoch_execution_time_observations_sharing_reason
                    .with_label_values(&["indebted"])
                    .inc();
            }

            if should_share {
                debug!("sharing new execution time observation for {key:?}: {new_average:?}");
                to_share.push((key, new_average));
                local_observation.last_shared = Some((new_average, Instant::now()));
            }
        }

        // Share new observations.
        self.share_observations(to_share);
    }

    fn generate_test_timings(
        &self,
        tx: &ProgrammableTransaction,
        timings: &[ExecutionTiming],
    ) -> (Vec<ExecutionTiming>, Duration) {
        let generated_timings: Vec<_> = tx
            .commands
            .iter()
            .zip(timings.iter())
            .map(|(command, timing)| {
                let key = ExecutionTimeObservationKey::from_command(command);
                let duration = self.get_test_duration(&key);
                if timing.is_abort() {
                    ExecutionTiming::Abort(duration)
                } else {
                    ExecutionTiming::Success(duration)
                }
            })
            .collect();

        let total_duration = generated_timings
            .iter()
            .map(|t| t.duration())
            .sum::<Duration>()
            + thread_rng().gen_range(Duration::from_millis(10)..Duration::from_millis(50));

        (generated_timings, total_duration)
    }

    fn get_test_duration(&self, key: &ExecutionTimeObservationKey) -> Duration {
        #[cfg(msim)]
        let should_inject = self.config.inject_synthetic_execution_time();
        #[cfg(not(msim))]
        let should_inject = false;

        if !in_test_configuration() && !should_inject {
            panic!("get_test_duration called in non-test configuration");
        }

        thread_local! {
            static PER_TEST_SEED: u64 = random::<u64>();
        }

        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        let checkpoint_digest_used = self
            .epoch_store
            .upgrade()
            .and_then(|store| {
                store
                    .get_lowest_non_genesis_checkpoint_summary()
                    .ok()
                    .flatten()
            })
            .map(|summary| summary.content_digest.hash(&mut hasher))
            .is_some();

        if !checkpoint_digest_used {
            PER_TEST_SEED.with(|seed| seed.hash(&mut hasher));
        }

        key.hash(&mut hasher);
        let mut rng = rngs::StdRng::seed_from_u64(hasher.finish());
        rng.gen_range(Duration::from_millis(100)..Duration::from_millis(600))
    }

    fn share_observations(&mut self, to_share: Vec<(ExecutionTimeObservationKey, Duration)>) {
        if to_share.is_empty() {
            return;
        }
        let Some(epoch_store) = self.epoch_store.upgrade() else {
            debug!("epoch is ending, dropping execution time observation");
            return;
        };

        let num_observations = to_share.len() as u64;

        // Enforce global observation-sharing rate limit.
        if let Err(e) = self.sharing_rate_limiter.check() {
            epoch_store
                .metrics
                .epoch_execution_time_observations_dropped
                .with_label_values(&["global_rate_limit"])
                .inc_by(num_observations);
            debug!("rate limit exceeded, dropping execution time observation; {e:?}");
            return;
        }

        let epoch_store = epoch_store.clone();
        let transaction = ConsensusTransaction::new_execution_time_observation(
            ExecutionTimeObservation::new(epoch_store.name, self.next_generation_number, to_share),
        );
        self.next_generation_number += 1;

        if let Err(e) = self.consensus_adapter.submit_best_effort(
            &transaction,
            &epoch_store,
            Duration::from_secs(5),
        ) {
            if !matches!(e.as_inner(), SuiErrorKind::EpochEnded(_)) {
                epoch_store
                    .metrics
                    .epoch_execution_time_observations_dropped
                    .with_label_values(&["submit_to_consensus"])
                    .inc_by(num_observations);
                warn!("failed to submit execution time observation: {e:?}");
            }
        } else {
            // Note: it is not actually guaranteed that the observation has been submitted at this point,
            // but that is also not true with ConsensusAdapter::submit_to_consensus. The only way to know
            // for sure is to observe that the message is processed by consensus handler.
            assert_reachable!("successfully shares execution time observations");
            epoch_store
                .metrics
                .epoch_execution_time_observations_shared
                .inc_by(num_observations);
        }
    }

    fn update_indebted_objects(&mut self, mut object_debts: Vec<ObjectID>) {
        let _scope = monitored_scope("ExecutionTimeObserver::update_indebted_objects");

        let Some(epoch_store) = self.epoch_store.upgrade() else {
            debug!("epoch is ending, dropping indebted object update");
            return;
        };

        object_debts.sort_unstable();
        object_debts.dedup();
        self.indebted_objects = object_debts;
        epoch_store
            .metrics
            .epoch_execution_time_observer_indebted_objects
            .set(self.indebted_objects.len() as i64);
    }
}

// Key used to save StoredExecutionTimeObservations in the Sui system state object's
// `extra_fields` Bag.
pub const EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY: u64 = 0;

// Tracks global execution time observations provided by validators from consensus
// and computes deterministic per-command estimates for use in congestion control.
pub struct ExecutionTimeEstimator {
    committee: Arc<Committee>,
    protocol_params: ExecutionTimeEstimateParams,

    consensus_observations: HashMap<ExecutionTimeObservationKey, ConsensusObservations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusObservations {
    observations: Vec<(u64 /* generation */, Option<Duration>)>, // keyed by authority index
    stake_weighted_median: Option<Duration>,                     // cached value
}

impl ConsensusObservations {
    fn update_stake_weighted_median(
        &mut self,
        committee: &Committee,
        config: &ExecutionTimeEstimateParams,
    ) {
        let mut stake_with_observations = 0;
        let sorted_observations: Vec<_> = self
            .observations
            .iter()
            .enumerate()
            .filter_map(|(i, (_, duration))| {
                duration.map(|duration| {
                    let authority_index: AuthorityIndex = i.try_into().unwrap();
                    stake_with_observations += committee.stake_by_index(authority_index).unwrap();
                    (duration, authority_index)
                })
            })
            .sorted()
            .collect();

        // Don't use observations until we have received enough.
        if stake_with_observations < config.stake_weighted_median_threshold {
            self.stake_weighted_median = None;
            return;
        }

        // Compute stake-weighted median.
        let median_stake = stake_with_observations / 2;
        let mut running_stake = 0;
        for (duration, authority_index) in sorted_observations {
            running_stake += committee.stake_by_index(authority_index).unwrap();
            if running_stake > median_stake {
                self.stake_weighted_median = Some(duration);
                break;
            }
        }
    }
}

impl ExecutionTimeEstimator {
    pub fn new(
        committee: Arc<Committee>,
        protocol_params: ExecutionTimeEstimateParams,
        initial_observations: impl Iterator<
            Item = (
                AuthorityIndex,
                Option<u64>,
                ExecutionTimeObservationKey,
                Duration,
            ),
        >,
    ) -> Self {
        let mut estimator = Self {
            committee,
            protocol_params,
            consensus_observations: HashMap::new(),
        };
        for (source, generation, key, duration) in initial_observations {
            estimator.process_observation_from_consensus(
                source,
                generation,
                key.to_owned(),
                duration,
                true,
            );
        }
        for observation in estimator.consensus_observations.values_mut() {
            observation
                .update_stake_weighted_median(&estimator.committee, &estimator.protocol_params);
        }
        estimator
    }

    #[cfg(test)]
    pub fn new_for_testing() -> Self {
        let (committee, _) = Committee::new_simple_test_committee_of_size(1);
        Self {
            committee: Arc::new(committee),
            protocol_params: ExecutionTimeEstimateParams {
                target_utilization: 100,
                max_estimate_us: u64::MAX,
                ..ExecutionTimeEstimateParams::default()
            },
            consensus_observations: HashMap::new(),
        }
    }

    pub fn process_observations_from_consensus(
        &mut self,
        source: AuthorityIndex,
        generation: Option<u64>,
        observations: &[(ExecutionTimeObservationKey, Duration)],
    ) {
        for (key, duration) in observations {
            self.process_observation_from_consensus(
                source,
                generation,
                key.to_owned(),
                *duration,
                false,
            );
        }
    }

    fn process_observation_from_consensus(
        &mut self,
        source: AuthorityIndex,
        generation: Option<u64>,
        observation_key: ExecutionTimeObservationKey,
        duration: Duration,
        skip_update: bool,
    ) {
        if matches!(observation_key, ExecutionTimeObservationKey::Publish) {
            // Special-case handling for Publish command: only use hard-coded default estimate.
            warn!(
                "dropping Publish observation received from possibly-Byzanitine authority {source}"
            );
            return;
        }

        assert_reachable!("receives some valid execution time observations");

        let observations = self
            .consensus_observations
            .entry(observation_key)
            .or_insert_with(|| {
                let len = self.committee.num_members();
                let mut empty_observations = Vec::with_capacity(len);
                empty_observations.resize(len, (0, None));
                ConsensusObservations {
                    observations: empty_observations,
                    stake_weighted_median: if self
                        .protocol_params
                        .default_none_duration_for_new_keys
                    {
                        None
                    } else {
                        Some(Duration::ZERO)
                    },
                }
            });

        let (obs_generation, obs_duration) =
            &mut observations.observations[TryInto::<usize>::try_into(source).unwrap()];
        if generation.is_some_and(|generation| *obs_generation >= generation) {
            // Ignore outdated observation.
            return;
        }
        *obs_generation = generation.unwrap_or(0);
        *obs_duration = Some(duration);
        if !skip_update {
            observations.update_stake_weighted_median(&self.committee, &self.protocol_params);
        }
    }

    pub fn get_estimate(&self, tx: &TransactionData) -> Duration {
        let TransactionKind::ProgrammableTransaction(tx) = tx.kind() else {
            debug_fatal!("get_estimate called on non-ProgrammableTransaction");
            return Duration::ZERO;
        };
        tx.commands
            .iter()
            .map(|command| {
                let key = ExecutionTimeObservationKey::from_command(command);
                self.consensus_observations
                    .get(&key)
                    .and_then(|obs| obs.stake_weighted_median)
                    .unwrap_or_else(|| key.default_duration())
                    // For native commands, adjust duration by length of command's inputs/outputs.
                    // This is sort of arbitrary, but hopefully works okay as a heuristic.
                    .mul_f64(command_length(command).get() as f64)
            })
            .sum::<Duration>()
            .min(Duration::from_micros(self.protocol_params.max_estimate_us))
    }

    pub fn take_observations(&mut self) -> StoredExecutionTimeObservations {
        StoredExecutionTimeObservations::V1(
            self.consensus_observations
                .drain()
                .map(|(key, observations)| {
                    let observations = observations
                        .observations
                        .into_iter()
                        .enumerate()
                        .filter_map(|(idx, (_, duration))| {
                            duration.map(|d| {
                                (
                                    self.committee
                                        .authority_by_index(idx.try_into().unwrap())
                                        .cloned()
                                        .unwrap(),
                                    d,
                                )
                            })
                        })
                        .collect();
                    (key, observations)
                })
                .collect(),
        )
    }

    pub fn get_observations(&self) -> Vec<(ExecutionTimeObservationKey, ConsensusObservations)> {
        self.consensus_observations
            .iter()
            .map(|(key, observations)| (key.clone(), observations.clone()))
            .collect()
    }
}

fn command_length(command: &Command) -> NonZeroUsize {
    // Commands with variable-length inputs/outputs are reported as +1
    // to account for fixed overhead and prevent divide-by-zero.
    NonZeroUsize::new(match command {
        Command::MoveCall(_) => 1,
        Command::TransferObjects(src, _) => src.len() + 1,
        Command::SplitCoins(_, amts) => amts.len() + 1,
        Command::MergeCoins(_, src) => src.len() + 1,
        Command::Publish(_, _) => 1,
        Command::MakeMoveVec(_, src) => src.len() + 1,
        Command::Upgrade(_, _, _, _) => 1,
    })
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::checkpoints::CheckpointStore;
    use crate::consensus_adapter::{
        ConnectionMonitorStatusForTests, ConsensusAdapter, ConsensusAdapterMetrics,
        MockConsensusClient,
    };
    use sui_protocol_config::ProtocolConfig;
    use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
    use sui_types::transaction::{
        Argument, CallArg, ObjectArg, ProgrammableMoveCall, SharedObjectMutability,
    };
    use {
        rand::{Rng, SeedableRng},
        sui_protocol_config::ProtocolVersion,
        sui_types::supported_protocol_versions::Chain,
    };

    #[tokio::test]
    async fn test_record_local_observations() {
        telemetry_subscribers::init_for_testing();

        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_per_object_congestion_control_mode_for_testing(
                PerObjectCongestionControlMode::ExecutionTimeEstimate(
                    ExecutionTimeEstimateParams {
                        target_utilization: 100,
                        allowed_txn_cost_overage_burst_limit_us: 0,
                        randomness_scalar: 100,
                        max_estimate_us: u64::MAX,
                        stored_observations_num_included_checkpoints: 10,
                        stored_observations_limit: u64::MAX,
                        stake_weighted_median_threshold: 0,
                        default_none_duration_for_new_keys: true,
                    },
                ),
            );
            config
        });

        let mock_consensus_client = MockConsensusClient::new();
        let authority = TestAuthorityBuilder::new().build().await;
        let epoch_store = authority.epoch_store_for_testing();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(mock_consensus_client),
            CheckpointStore::new_for_tests(),
            authority.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
        ));
        let mut observer = ExecutionTimeObserver::new_for_testing(
            epoch_store.clone(),
            Box::new(consensus_adapter.clone()),
            Duration::ZERO, // disable object utilization thresholds for this test
            false,          // disable gas price weighting for this test
        );

        // Create a simple PTB with one move call
        let package = ObjectID::random();
        let module = "test_module".to_string();
        let function = "test_function".to_string();
        let ptb = ProgrammableTransaction {
            inputs: vec![],
            commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
                package,
                module: module.clone(),
                function: function.clone(),
                type_arguments: vec![],
                arguments: vec![],
            }))],
        };

        // Record an observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(100))];
        let total_duration = Duration::from_millis(110);
        observer.record_local_observations(&ptb, &timings, total_duration, 1);

        let key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };

        // Check that local observation was recorded and shared
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.get_average(),
            // 10ms overhead should be entirely apportioned to the one command in the PTB
            Duration::from_millis(110)
        );
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(110));

        // Record another observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(110))];
        let total_duration = Duration::from_millis(120);
        observer.record_local_observations(&ptb, &timings, total_duration, 1);

        // Check that moving average was updated
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.get_average(),
            // average of 110ms and 120ms observations
            Duration::from_millis(115)
        );
        // new 115ms average should not be shared; it's <5% different from 110ms
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(110));

        // Record another observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(120))];
        let total_duration = Duration::from_millis(130);
        observer.record_local_observations(&ptb, &timings, total_duration, 1);

        // Check that moving average was updated
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.get_average(),
            // average of [110ms, 120ms, 130ms]
            Duration::from_millis(120)
        );
        // new 120ms average should not be shared; it's >5% different from 110ms,
        // but not enough time has passed
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(110));

        // Manually update last-shared time to long ago
        observer
            .local_observations
            .get_mut(&key)
            .unwrap()
            .last_shared = Some((
            Duration::from_millis(110),
            Instant::now() - Duration::from_secs(60),
        ));

        // Record last observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(120))];
        let total_duration = Duration::from_millis(160);
        observer.record_local_observations(&ptb, &timings, total_duration, 1);

        // Verify that moving average is the same and a new observation was shared, as
        // enough time has now elapsed
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.get_average(),
            // average of [110ms, 120ms, 130ms, 160ms]
            Duration::from_millis(130)
        );
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(130));
    }

    #[tokio::test]
    async fn test_record_local_observations_with_gas_price_weighting() {
        telemetry_subscribers::init_for_testing();

        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_per_object_congestion_control_mode_for_testing(
                PerObjectCongestionControlMode::ExecutionTimeEstimate(
                    ExecutionTimeEstimateParams {
                        target_utilization: 100,
                        allowed_txn_cost_overage_burst_limit_us: 0,
                        randomness_scalar: 100,
                        max_estimate_us: u64::MAX,
                        stored_observations_num_included_checkpoints: 10,
                        stored_observations_limit: u64::MAX,
                        stake_weighted_median_threshold: 0,
                        default_none_duration_for_new_keys: true,
                    },
                ),
            );
            config
        });

        let mock_consensus_client = MockConsensusClient::new();
        let authority = TestAuthorityBuilder::new().build().await;
        let epoch_store = authority.epoch_store_for_testing();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(mock_consensus_client),
            CheckpointStore::new_for_tests(),
            authority.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
        ));
        let mut observer = ExecutionTimeObserver::new_for_testing(
            epoch_store.clone(),
            Box::new(consensus_adapter.clone()),
            Duration::ZERO, // disable object utilization thresholds for this test
            true,           // enable gas price weighting for this test
        );

        // Create a simple PTB with one move call
        let package = ObjectID::random();
        let module = "test_module".to_string();
        let function = "test_function".to_string();
        let ptb = ProgrammableTransaction {
            inputs: vec![],
            commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
                package,
                module: module.clone(),
                function: function.clone(),
                type_arguments: vec![],
                arguments: vec![],
            }))],
        };

        // Record an observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(100))];
        let total_duration = Duration::from_millis(110);
        observer.record_local_observations(&ptb, &timings, total_duration, 1);

        let key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };

        // Check that local observation was recorded and shared
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.get_average(),
            // 10ms overhead should be entirely apportioned to the one command in the PTB
            Duration::from_millis(110)
        );
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(110));

        // Record another observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(110))];
        let total_duration = Duration::from_millis(120);
        observer.record_local_observations(&ptb, &timings, total_duration, 2);

        // Check that weighted moving average was updated
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.get_average(),
            // Our local observation averages are weighted by gas price:
            // 110ms * 1 + 110ms * 2 / (1 + 2) = 116.666ms
            Duration::from_micros(116_666)
        );
    }

    #[tokio::test]
    async fn test_record_local_observations_with_multiple_commands() {
        telemetry_subscribers::init_for_testing();

        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_per_object_congestion_control_mode_for_testing(
                PerObjectCongestionControlMode::ExecutionTimeEstimate(
                    ExecutionTimeEstimateParams {
                        target_utilization: 100,
                        allowed_txn_cost_overage_burst_limit_us: 0,
                        randomness_scalar: 0,
                        max_estimate_us: u64::MAX,
                        stored_observations_num_included_checkpoints: 10,
                        stored_observations_limit: u64::MAX,
                        stake_weighted_median_threshold: 0,
                        default_none_duration_for_new_keys: true,
                    },
                ),
            );
            config
        });

        let mock_consensus_client = MockConsensusClient::new();
        let authority = TestAuthorityBuilder::new().build().await;
        let epoch_store = authority.epoch_store_for_testing();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(mock_consensus_client),
            CheckpointStore::new_for_tests(),
            authority.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
        ));
        let mut observer = ExecutionTimeObserver::new_for_testing(
            epoch_store.clone(),
            Box::new(consensus_adapter.clone()),
            Duration::ZERO, // disable object utilization thresholds for this test
            false,          // disable gas price weighting for this test
        );

        // Create a PTB with multiple commands.
        let package = ObjectID::random();
        let module = "test_module".to_string();
        let function = "test_function".to_string();
        let ptb = ProgrammableTransaction {
            inputs: vec![],
            commands: vec![
                Command::MoveCall(Box::new(ProgrammableMoveCall {
                    package,
                    module: module.clone(),
                    function: function.clone(),
                    type_arguments: vec![],
                    arguments: vec![],
                })),
                Command::TransferObjects(
                    // Inputs don't exist above, but doesn't matter for this test.
                    vec![Argument::Input(1), Argument::Input(2)],
                    Argument::Input(0),
                ),
            ],
        };
        let timings = vec![
            ExecutionTiming::Success(Duration::from_millis(100)),
            ExecutionTiming::Success(Duration::from_millis(50)),
        ];
        let total_duration = Duration::from_millis(180);
        observer.record_local_observations(&ptb, &timings, total_duration, 1);

        // Check that both commands were recorded
        let move_key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };
        let move_obs = observer.local_observations.get(&move_key).unwrap();
        assert_eq!(
            move_obs.get_average(),
            // 100/150 == 2/3 of 30ms overhead distributed to Move command
            Duration::from_millis(120)
        );

        let transfer_obs = observer
            .local_observations
            .get(&ExecutionTimeObservationKey::TransferObjects)
            .unwrap();
        assert_eq!(
            transfer_obs.get_average(),
            // 50ms time before adjustments
            // 50/150 == 1/3 of 30ms overhead distributed to object xfer
            // 60ms adjusetd time / 3 command length == 20ms
            Duration::from_millis(20)
        );
    }

    #[tokio::test]
    async fn test_record_local_observations_with_object_utilization_threshold() {
        telemetry_subscribers::init_for_testing();

        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_per_object_congestion_control_mode_for_testing(
                PerObjectCongestionControlMode::ExecutionTimeEstimate(
                    ExecutionTimeEstimateParams {
                        target_utilization: 100,
                        allowed_txn_cost_overage_burst_limit_us: 0,
                        randomness_scalar: 0,
                        max_estimate_us: u64::MAX,
                        stored_observations_num_included_checkpoints: 10,
                        stored_observations_limit: u64::MAX,
                        stake_weighted_median_threshold: 0,
                        default_none_duration_for_new_keys: true,
                    },
                ),
            );
            config
        });

        let mock_consensus_client = MockConsensusClient::new();
        let authority = TestAuthorityBuilder::new().build().await;
        let epoch_store = authority.epoch_store_for_testing();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(mock_consensus_client),
            CheckpointStore::new_for_tests(),
            authority.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
        ));
        let mut observer = ExecutionTimeObserver::new_for_testing(
            epoch_store.clone(),
            Box::new(consensus_adapter.clone()),
            Duration::from_millis(500), // only share observations with excess utilization >= 500ms
            false,                      // disable gas price weighting for this test
        );

        // Create a simple PTB with one move call and one mutable shared input
        let package = ObjectID::random();
        let module = "test_module".to_string();
        let function = "test_function".to_string();
        let shared_object_id = ObjectID::random();
        let ptb = ProgrammableTransaction {
            inputs: vec![CallArg::Object(ObjectArg::SharedObject {
                id: shared_object_id,
                initial_shared_version: SequenceNumber::new(),
                mutability: SharedObjectMutability::Mutable,
            })],
            commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
                package,
                module: module.clone(),
                function: function.clone(),
                type_arguments: vec![],
                arguments: vec![],
            }))],
        };
        let key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };

        tokio::time::pause();

        // First observation - should not share due to low utilization
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(1))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(2), 1);
        assert!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .is_none()
        );

        // Second observation - no time has passed, so now utilization is high; should share upward change
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(1))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(2), 1);
        assert_eq!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .unwrap()
                .0,
            Duration::from_secs(2)
        );

        // Third execution with significant upward diff and high utilization - should share again
        tokio::time::advance(Duration::from_secs(5)).await;
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(3))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(5), 1);
        assert_eq!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .unwrap()
                .0,
            Duration::from_secs(3)
        );

        // Fourth execution with significant downward diff but still overutilized - should NOT share downward change
        // (downward changes are only shared for indebted objects, not overutilized ones)
        tokio::time::advance(Duration::from_millis(150)).await;
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(100))];
        observer.record_local_observations(&ptb, &timings, Duration::from_millis(500), 1);
        assert_eq!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .unwrap()
                .0,
            Duration::from_secs(3) // still the old value, no sharing of downward change
        );

        // Fifth execution after utilization drops - should not share upward diff since not overutilized
        tokio::time::advance(Duration::from_secs(60)).await;
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(11))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(11), 1);
        assert_eq!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .unwrap()
                .0,
            Duration::from_secs(3) // still the old value, no sharing when not overutilized
        );
    }

    #[tokio::test]
    async fn test_record_local_observations_with_indebted_objects() {
        telemetry_subscribers::init_for_testing();

        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_per_object_congestion_control_mode_for_testing(
                PerObjectCongestionControlMode::ExecutionTimeEstimate(
                    ExecutionTimeEstimateParams {
                        target_utilization: 100,
                        allowed_txn_cost_overage_burst_limit_us: 0,
                        randomness_scalar: 0,
                        max_estimate_us: u64::MAX,
                        stored_observations_num_included_checkpoints: 10,
                        stored_observations_limit: u64::MAX,
                        stake_weighted_median_threshold: 0,
                        default_none_duration_for_new_keys: true,
                    },
                ),
            );
            config
        });

        let mock_consensus_client = MockConsensusClient::new();
        let authority = TestAuthorityBuilder::new().build().await;
        let epoch_store = authority.epoch_store_for_testing();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(mock_consensus_client),
            CheckpointStore::new_for_tests(),
            authority.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
        ));
        let mut observer = ExecutionTimeObserver::new_for_testing(
            epoch_store.clone(),
            Box::new(consensus_adapter.clone()),
            Duration::from_millis(500), // Low utilization threshold to enable overutilized sharing initially
            false,                      // disable gas price weighting for this test
        );

        // Create a simple PTB with one move call and one mutable shared input
        let package = ObjectID::random();
        let module = "test_module".to_string();
        let function = "test_function".to_string();
        let shared_object_id = ObjectID::random();
        let ptb = ProgrammableTransaction {
            inputs: vec![CallArg::Object(ObjectArg::SharedObject {
                id: shared_object_id,
                initial_shared_version: SequenceNumber::new(),
                mutability: SharedObjectMutability::Mutable,
            })],
            commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
                package,
                module: module.clone(),
                function: function.clone(),
                type_arguments: vec![],
                arguments: vec![],
            }))],
        };
        let key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };

        tokio::time::pause();

        // First observation - should not share due to low utilization
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(1))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(1), 1);
        assert!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .is_none()
        );

        // Second observation - no time has passed, so now utilization is high; should share upward change
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(2))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(2), 1);
        assert_eq!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .unwrap()
                .0,
            Duration::from_millis(1500) // (1s + 2s) / 2 = 1.5s
        );

        // Mark the shared object as indebted and increase utilization threshold to prevent overutilized sharing
        observer.update_indebted_objects(vec![shared_object_id]);
        observer
            .config
            .observation_sharing_object_utilization_threshold = Some(Duration::from_secs(1000));

        // Wait for min interval and record a significant downward change
        // This should share because the object is indebted
        tokio::time::advance(Duration::from_secs(60)).await;
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(300))];
        observer.record_local_observations(&ptb, &timings, Duration::from_millis(300), 1);

        // Moving average should be (1s + 2s + 0.3s) / 3 = 1.1s
        // This downward change should have been shared for indebted object
        assert_eq!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .unwrap()
                .0,
            Duration::from_millis(1100)
        );
    }

    #[tokio::test]
    // TODO-DNS add tests for min stake amt
    async fn test_stake_weighted_median() {
        telemetry_subscribers::init_for_testing();

        let (committee, _) =
            Committee::new_simple_test_committee_with_normalized_voting_power(vec![10, 20, 30, 40]);

        let params = ExecutionTimeEstimateParams {
            stake_weighted_median_threshold: 0,
            ..Default::default()
        };

        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, Some(Duration::from_secs(2))), // 20% stake
                (0, Some(Duration::from_secs(3))), // 30% stake
                (0, Some(Duration::from_secs(4))), // 40% stake
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        // With stake weights [10,20,30,40]:
        // - Duration 1 covers 10% of stake
        // - Duration 2 covers 30% of stake (10+20)
        // - Duration 3 covers 60% of stake (10+20+30)
        // - Duration 4 covers 100% of stake
        // Median should be 3 since that's where we cross 50% of stake
        assert_eq!(tracker.stake_weighted_median, Some(Duration::from_secs(3)));

        // Test duration sorting
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(3))), // 10% stake
                (0, Some(Duration::from_secs(4))), // 20% stake
                (0, Some(Duration::from_secs(1))), // 30% stake
                (0, Some(Duration::from_secs(2))), // 40% stake
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        // With sorted stake weights [30,40,10,20]:
        // - Duration 1 covers 30% of stake
        // - Duration 2 covers 70% of stake (30+40)
        // - Duration 3 covers 80% of stake (30+40+10)
        // - Duration 4 covers 100% of stake
        // Median should be 2 since that's where we cross 50% of stake
        assert_eq!(tracker.stake_weighted_median, Some(Duration::from_secs(2)));

        // Test with one missing observation
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, None),                         // 20% stake (missing)
                (0, Some(Duration::from_secs(3))), // 30% stake
                (0, Some(Duration::from_secs(4))), // 40% stake
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        // With missing observation for 20% stake:
        // - Duration 1 covers 10% of stake
        // - Duration 3 covers 40% of stake (10+30)
        // - Duration 4 covers 80% of stake (10+30+40)
        // Median should be 4 since that's where we pass half of available stake (80% / 2 == 40%)
        assert_eq!(tracker.stake_weighted_median, Some(Duration::from_secs(4)));

        // Test with multiple missing observations
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, Some(Duration::from_secs(2))), // 20% stake
                (0, None),                         // 30% stake (missing)
                (0, None),                         // 40% stake (missing)
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        // With missing observations:
        // - Duration 1 covers 10% of stake
        // - Duration 2 covers 30% of stake (10+20)
        // Median should be 2 since that's where we cross half of available stake (40% / 2 == 20%)
        assert_eq!(tracker.stake_weighted_median, Some(Duration::from_secs(2)));

        // Test with one observation
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, None),                         // 10% stake
                (0, None),                         // 20% stake
                (0, Some(Duration::from_secs(3))), // 30% stake
                (0, None),                         // 40% stake
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        // With only one observation, median should be that observation
        assert_eq!(tracker.stake_weighted_median, Some(Duration::from_secs(3)));

        // Test with all same durations
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(5))), // 10% stake
                (0, Some(Duration::from_secs(5))), // 20% stake
                (0, Some(Duration::from_secs(5))), // 30% stake
                (0, Some(Duration::from_secs(5))), // 40% stake
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        assert_eq!(tracker.stake_weighted_median, Some(Duration::from_secs(5)));
    }

    #[tokio::test]
    async fn test_stake_weighted_median_threshold() {
        telemetry_subscribers::init_for_testing();

        let (committee, _) =
            Committee::new_simple_test_committee_with_normalized_voting_power(vec![10, 20, 30, 40]);

        // Test with threshold requiring at least 50% stake
        let params = ExecutionTimeEstimateParams {
            stake_weighted_median_threshold: 5000,
            ..Default::default()
        };

        // Test with insufficient stake (only 30% have observations)
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, Some(Duration::from_secs(2))), // 20% stake
                (0, None),                         // 30% stake (missing)
                (0, None),                         // 40% stake (missing)
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        // Should not compute median since only 30% stake has observations (< 50% threshold)
        assert_eq!(tracker.stake_weighted_median, None);

        // Test with sufficient stake (60% have observations)
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, Some(Duration::from_secs(2))), // 20% stake
                (0, Some(Duration::from_secs(3))), // 30% stake
                (0, None),                         // 40% stake (missing)
            ],
            stake_weighted_median: None,
        };
        tracker.update_stake_weighted_median(&committee, &params);
        // Should compute median since 60% stake has observations (>= 50% threshold)
        assert_eq!(tracker.stake_weighted_median, Some(Duration::from_secs(3)));
    }

    #[tokio::test]
    async fn test_execution_time_estimator() {
        telemetry_subscribers::init_for_testing();

        let (committee, _) =
            Committee::new_simple_test_committee_with_normalized_voting_power(vec![10, 20, 30, 40]);
        let mut estimator = ExecutionTimeEstimator::new(
            Arc::new(committee),
            ExecutionTimeEstimateParams {
                target_utilization: 50,
                max_estimate_us: 1_500_000,

                // Not used in this test.
                allowed_txn_cost_overage_burst_limit_us: 0,
                randomness_scalar: 0,
                stored_observations_num_included_checkpoints: 10,
                stored_observations_limit: u64::MAX,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: true,
            },
            std::iter::empty(),
        );
        // Create test keys
        let package = ObjectID::random();
        let module = "test_module".to_string();
        let function = "test_function".to_string();
        let move_key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };
        let transfer_key = ExecutionTimeObservationKey::TransferObjects;

        // Record observations from different validators
        // First record some old observations that should be ignored
        estimator.process_observation_from_consensus(
            0,
            Some(1),
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            Some(1),
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            Some(1),
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );

        estimator.process_observation_from_consensus(
            0,
            Some(1),
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            Some(1),
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            Some(1),
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );

        // Now record newer observations that should be used
        estimator.process_observation_from_consensus(
            0,
            Some(2),
            move_key.clone(),
            Duration::from_millis(100),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            Some(2),
            move_key.clone(),
            Duration::from_millis(200),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            Some(2),
            move_key.clone(),
            Duration::from_millis(300),
            false,
        );

        estimator.process_observation_from_consensus(
            0,
            Some(2),
            transfer_key.clone(),
            Duration::from_millis(50),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            Some(2),
            transfer_key.clone(),
            Duration::from_millis(60),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            Some(2),
            transfer_key.clone(),
            Duration::from_millis(70),
            false,
        );

        // Try to record old observations again - these should be ignored
        estimator.process_observation_from_consensus(
            0,
            Some(1),
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            Some(1),
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            Some(1),
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );

        // Test single command transaction
        let single_move_tx = TransactionData::new_programmable(
            SuiAddress::ZERO,
            vec![],
            ProgrammableTransaction {
                inputs: vec![],
                commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
                    package,
                    module: module.clone(),
                    function: function.clone(),
                    type_arguments: vec![],
                    arguments: vec![],
                }))],
            },
            100,
            100,
        );

        // Should return median of move call observations (300ms)
        assert_eq!(
            estimator.get_estimate(&single_move_tx),
            Duration::from_millis(300)
        );

        // Test multi-command transaction
        let multi_command_tx = TransactionData::new_programmable(
            SuiAddress::ZERO,
            vec![],
            ProgrammableTransaction {
                inputs: vec![],
                commands: vec![
                    Command::MoveCall(Box::new(ProgrammableMoveCall {
                        package,
                        module: module.clone(),
                        function: function.clone(),
                        type_arguments: vec![],
                        arguments: vec![],
                    })),
                    Command::TransferObjects(
                        vec![Argument::Input(1), Argument::Input(2)],
                        Argument::Input(0),
                    ),
                ],
            },
            100,
            100,
        );

        // Should return sum of median move call (300ms)
        // plus the median transfer (70ms) * command length (3)
        assert_eq!(
            estimator.get_estimate(&multi_command_tx),
            Duration::from_millis(510)
        );
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct ExecutionTimeObserverSnapshot {
        protocol_version: u64,
        consensus_observations: Vec<(ExecutionTimeObservationKey, ConsensusObservations)>,
        transaction_estimates: Vec<(String, Duration)>, // (transaction_description, estimated_duration)
    }

    fn generate_test_inputs(
        seed: u64,
        num_validators: usize,
        generation_override: Option<u64>,
    ) -> Vec<(
        AuthorityIndex,
        Option<u64>,
        ExecutionTimeObservationKey,
        Duration,
    )> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        let observation_keys = vec![
            ExecutionTimeObservationKey::MoveEntryPoint {
                package: ObjectID::from_hex_literal("0x1").unwrap(),
                module: "coin".to_string(),
                function: "transfer".to_string(),
                type_arguments: vec![],
            },
            ExecutionTimeObservationKey::MoveEntryPoint {
                package: ObjectID::from_hex_literal("0x2").unwrap(),
                module: "nft".to_string(),
                function: "mint".to_string(),
                type_arguments: vec![],
            },
            ExecutionTimeObservationKey::TransferObjects,
            ExecutionTimeObservationKey::SplitCoins,
            ExecutionTimeObservationKey::MergeCoins,
            ExecutionTimeObservationKey::MakeMoveVec,
            ExecutionTimeObservationKey::Upgrade,
        ];

        let mut inputs = Vec::new();
        let target_samples = 25;

        for _ in 0..target_samples {
            let key = observation_keys[rng.gen_range(0..observation_keys.len())].clone();
            let authority_index =
                AuthorityIndex::try_from(rng.gen_range(0..num_validators)).unwrap();

            // Use realistic range where newer generations might replace older ones
            let generation = generation_override.unwrap_or_else(|| rng.gen_range(1..=10));

            // Generate duration based on key type with realistic variance
            // Sometimes generate zero values to test corner cases with byzantine validators
            let base_duration = if rng.gen_ratio(1, 20) {
                // 5% chance of zero duration to test corner cases
                0
            } else {
                match &key {
                    ExecutionTimeObservationKey::MoveEntryPoint { .. } => rng.gen_range(50..=500),
                    ExecutionTimeObservationKey::TransferObjects => rng.gen_range(10..=100),
                    ExecutionTimeObservationKey::SplitCoins => rng.gen_range(20..=80),
                    ExecutionTimeObservationKey::MergeCoins => rng.gen_range(15..=70),
                    ExecutionTimeObservationKey::MakeMoveVec => rng.gen_range(5..=30),
                    ExecutionTimeObservationKey::Upgrade => rng.gen_range(100..=1000),
                    ExecutionTimeObservationKey::Publish => rng.gen_range(200..=2000),
                }
            };

            let duration = Duration::from_millis(base_duration);

            inputs.push((authority_index, Some(generation), key, duration));
        }

        inputs
    }

    fn generate_test_transactions(seed: u64) -> Vec<(String, TransactionData)> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut transactions = Vec::new();

        let package3 = ObjectID::from_hex_literal("0x3").unwrap();
        transactions.push((
            "coin_transfer_call".to_string(),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
                        package: ObjectID::from_hex_literal("0x1").unwrap(),
                        module: "coin".to_string(),
                        function: "transfer".to_string(),
                        type_arguments: vec![],
                        arguments: vec![],
                    }))],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        transactions.push((
            "mixed_move_calls".to_string(),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![
                        Command::MoveCall(Box::new(ProgrammableMoveCall {
                            package: ObjectID::from_hex_literal("0x1").unwrap(),
                            module: "coin".to_string(),
                            function: "transfer".to_string(),
                            type_arguments: vec![],
                            arguments: vec![],
                        })),
                        Command::MoveCall(Box::new(ProgrammableMoveCall {
                            package: ObjectID::from_hex_literal("0x2").unwrap(),
                            module: "nft".to_string(),
                            function: "mint".to_string(),
                            type_arguments: vec![],
                            arguments: vec![],
                        })),
                    ],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        transactions.push((
            "native_commands_with_observations".to_string(),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![
                        Command::TransferObjects(vec![Argument::Input(0)], Argument::Input(1)),
                        Command::SplitCoins(Argument::Input(2), vec![Argument::Input(3)]),
                        Command::MergeCoins(Argument::Input(4), vec![Argument::Input(5)]),
                        Command::MakeMoveVec(None, vec![Argument::Input(6)]),
                    ],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        let num_objects = rng.gen_range(1..=5);
        transactions.push((
            format!("transfer_objects_{}_items", num_objects),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![Command::TransferObjects(
                        (0..num_objects).map(Argument::Input).collect(),
                        Argument::Input(num_objects),
                    )],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        let num_amounts = rng.gen_range(1..=4);
        transactions.push((
            format!("split_coins_{}_amounts", num_amounts),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![Command::SplitCoins(
                        Argument::Input(0),
                        (1..=num_amounts).map(Argument::Input).collect(),
                    )],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        let num_sources = rng.gen_range(1..=3);
        transactions.push((
            format!("merge_coins_{}_sources", num_sources),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![Command::MergeCoins(
                        Argument::Input(0),
                        (1..=num_sources).map(Argument::Input).collect(),
                    )],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        let num_elements = rng.gen_range(0..=6);
        transactions.push((
            format!("make_move_vec_{}_elements", num_elements),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![Command::MakeMoveVec(
                        None,
                        (0..num_elements).map(Argument::Input).collect(),
                    )],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        transactions.push((
            "mixed_commands".to_string(),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![
                        Command::MoveCall(Box::new(ProgrammableMoveCall {
                            package: package3,
                            module: "game".to_string(),
                            function: "play".to_string(),
                            type_arguments: vec![],
                            arguments: vec![],
                        })),
                        Command::TransferObjects(
                            vec![Argument::Input(1), Argument::Input(2)],
                            Argument::Input(0),
                        ),
                        Command::SplitCoins(Argument::Input(3), vec![Argument::Input(4)]),
                    ],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        transactions.push((
            "upgrade_package".to_string(),
            TransactionData::new_programmable(
                SuiAddress::ZERO,
                vec![],
                ProgrammableTransaction {
                    inputs: vec![],
                    commands: vec![Command::Upgrade(
                        vec![],
                        vec![],
                        package3,
                        Argument::Input(0),
                    )],
                },
                rng.gen_range(100..1000),
                rng.gen_range(100..1000),
            ),
        ));

        transactions
    }

    // Safeguard against forking because of changes to the execution time estimator.
    //
    // Within an epoch, each estimator must reach the same conclusion about the observations and
    // stake_weighted_median from the observations shared by other validators, as this is used
    // for transaction ordering.
    //
    // Therefore; any change in the calculation of the observations or stake_weighted_median
    // not accompanied by a protocol version change may fork.
    //
    // This test uses snapshots of computed stake weighted median at particular protocol versions
    // to attempt to discover regressions that might fork.
    #[test]
    fn snapshot_tests() {
        println!("\n============================================================================");
        println!("!                                                                          !");
        println!("! IMPORTANT: never update snapshots from this test. only add new versions! !");
        println!("!                                                                          !");
        println!("============================================================================\n");

        let max_version = ProtocolVersion::MAX.as_u64();

        let test_versions: Vec<u64> = (max_version.saturating_sub(9)..=max_version).collect();

        for version in test_versions {
            let protocol_version = ProtocolVersion::new(version);
            let protocol_config = ProtocolConfig::get_for_version(protocol_version, Chain::Unknown);
            let (committee, _) = Committee::new_simple_test_committee_of_size(4);
            let committee = Arc::new(committee);

            let initial_generation =
                if let PerObjectCongestionControlMode::ExecutionTimeEstimate(params) =
                    protocol_config.per_object_congestion_control_mode()
                {
                    if params.default_none_duration_for_new_keys {
                        None
                    } else {
                        Some(0)
                    }
                } else {
                    Some(0) // fallback for versions without execution time estimate mode
                };

            let initial_observations =
                generate_test_inputs(0, committee.num_members(), initial_generation);
            let mut estimator = ExecutionTimeEstimator::new(
                committee.clone(),
                ExecutionTimeEstimateParams {
                    max_estimate_us: u64::MAX, // Allow unlimited estimates for testing
                    ..ExecutionTimeEstimateParams::default()
                },
                initial_observations.into_iter(),
            );

            let test_inputs = generate_test_inputs(version, committee.num_members(), None);

            for (source, generation, observation_key, duration) in test_inputs {
                estimator.process_observation_from_consensus(
                    source,
                    generation,
                    observation_key,
                    duration,
                    false,
                );
            }

            let mut final_observations = estimator.get_observations();
            final_observations.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()));

            let test_transactions = generate_test_transactions(version);
            let mut transaction_estimates = Vec::new();
            for (description, tx_data) in test_transactions {
                let estimate = estimator.get_estimate(&tx_data);
                transaction_estimates.push((description, estimate));
            }

            let snapshot_data = ExecutionTimeObserverSnapshot {
                protocol_version: version,
                consensus_observations: final_observations.clone(),
                transaction_estimates,
            };
            insta::assert_yaml_snapshot!(
                format!("execution_time_observer_v{}", version),
                snapshot_data
            );
        }
    }
}
