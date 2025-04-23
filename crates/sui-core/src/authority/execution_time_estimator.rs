// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Weak},
    time::Duration,
};

use super::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::SubmitToConsensus;
use governor::{clock::MonotonicClock, Quota, RateLimiter};
use itertools::Itertools;
use lru::LruCache;
use mysten_common::{assert_reachable, debug_fatal};
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use simple_moving_average::{SingleSumSMA, SMA};
use sui_config::node::ExecutionTimeObserverConfig;
use sui_protocol_config::{ExecutionTimeEstimateParams, PerObjectCongestionControlMode};
use sui_types::{
    base_types::ObjectID,
    committee::Committee,
    error::SuiError,
    execution::{ExecutionTimeObservationKey, ExecutionTiming},
    messages_consensus::{AuthorityIndex, ConsensusTransaction, ExecutionTimeObservation},
    transaction::{
        Command, ProgrammableTransaction, StoredExecutionTimeObservations, TransactionData,
        TransactionDataAPI, TransactionKind,
    },
};
use tokio::{sync::mpsc, time::Instant};
use tracing::{debug, info, warn};

// TODO: Move this into ExecutionTimeObserverConfig, if we switch to a moving average
// implmentation without the window size in the type.
const LOCAL_OBSERVATION_WINDOW_SIZE: usize = 10;

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
}

#[derive(Debug, Clone)]
pub struct LocalObservations {
    moving_average: SingleSumSMA<Duration, u32, LOCAL_OBSERVATION_WINDOW_SIZE>,
    last_shared: Option<(Duration, Instant)>,
}

#[derive(Debug, Clone)]
pub struct ObjectUtilization {
    excess_execution_time: Duration,
    last_measured: Option<Instant>,
    was_overutilized: bool, // true if the object has ever had excess_execution_time
}

impl ObjectUtilization {
    pub fn overutilized(&self) -> bool {
        self.excess_execution_time > Duration::ZERO
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
            info!("ExecutionTimeObserver disabled because per-object congestion control mode is not ExecutionTimeEstimate");
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
        };
        spawn_monitored_task!(epoch_store.within_alive_epoch(async move {
            loop {
                tokio::select! {
                    // TODO: add metrics for messages received.
                    Some(object_debts) = rx_object_debts.recv() => {
                        observer.update_indebted_objects(object_debts);
                    }
                    Some((tx, timings, total_duration)) = rx_local_execution_time.recv() => {
                        observer
                            .record_local_observations(&tx, &timings, total_duration);
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
    ) -> Self {
        let PerObjectCongestionControlMode::ExecutionTimeEstimate(protocol_params) = epoch_store
            .protocol_config()
            .per_object_congestion_control_mode()
        else {
            panic!("tried to construct test ExecutionTimeObserver when congestion control mode is not ExecutionTimeEstimate");
        };
        Self {
            epoch_store: Arc::downgrade(&epoch_store),
            consensus_adapter,
            protocol_params,
            config: ExecutionTimeObserverConfig {
                observation_sharing_object_utilization_threshold: Some(
                    observation_sharing_object_utilization_threshold,
                ),
                ..ExecutionTimeObserverConfig::default()
            },
            local_observations: LruCache::new(NonZeroUsize::new(10000).unwrap()),
            object_utilization_tracker: LruCache::new(NonZeroUsize::new(50000).unwrap()),
            indebted_objects: Vec::new(),
            sharing_rate_limiter: RateLimiter::direct_with_clock(
                Quota::per_hour(std::num::NonZeroU32::MAX),
                &MonotonicClock,
            ),
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
    ) {
        let _scope = monitored_scope("ExecutionTimeObserver::record_local_observations");

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
            .filter_map(|obj| obj.mutable.then_some(obj.id))
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
                let overutilized_at_start = utilization.overutilized();
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
                if utilization.excess_execution_time > Duration::ZERO {
                    utilization.was_overutilized = true;
                }

                // Update overutilized objects metrics.
                if !overutilized_at_start && utilization.overutilized() {
                    epoch_store
                        .metrics
                        .epoch_execution_time_observer_overutilized_objects
                        .inc();
                } else if overutilized_at_start && !utilization.overutilized() {
                    epoch_store
                        .metrics
                        .epoch_execution_time_observer_overutilized_objects
                        .dec();
                }
                if self.config.report_object_utilization_metric() && utilization.was_overutilized {
                    epoch_store
                        .metrics
                        .epoch_execution_time_observer_object_utilization
                        .with_label_values(&[id.to_string().as_str()])
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

            // Update moving-average observation for the command.
            let key = ExecutionTimeObservationKey::from_command(command);
            let local_observation =
                self.local_observations
                    .get_or_insert_mut(key.clone(), || LocalObservations {
                        moving_average: SingleSumSMA::from_zero(Duration::ZERO),
                        last_shared: None,
                    });
            local_observation
                .moving_average
                .add_sample(command_duration);

            // Send a new observation through consensus if:
            // - our current moving average differs too much from the last one we shared, and
            // - the tx has at least one mutable shared object with utilization that's too high
            // TODO: Consider only sharing observations that disagree with consensus estimate.
            let new_average = local_observation.moving_average.get_average();
            let diff_exceeds_threshold =
                local_observation
                    .last_shared
                    .is_none_or(|(last_shared, last_shared_timestamp)| {
                        let diff = last_shared.abs_diff(new_average);
                        diff >= new_average
                            .mul_f64(self.config.observation_sharing_diff_threshold())
                            && last_shared_timestamp.elapsed()
                                >= self.config.observation_sharing_min_interval()
                    });
            let utilization_exceeds_threshold = max_excess_per_object_execution_time
                >= self
                    .config
                    .observation_sharing_object_utilization_threshold();
            if diff_exceeds_threshold && (utilization_exceeds_threshold || uses_indebted_object) {
                debug!("sharing new execution time observation for {key:?}: {new_average:?}");
                to_share.push((key, new_average));
                local_observation.last_shared = Some((new_average, Instant::now()));
            }
        }

        // Share new observations.
        self.share_observations(to_share);
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
            ExecutionTimeObservation::new(epoch_store.name, to_share),
        );

        if let Err(e) = self.consensus_adapter.submit_best_effort(
            &transaction,
            &epoch_store,
            Duration::from_secs(5),
        ) {
            if !matches!(e, SuiError::EpochEnded(_)) {
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

#[derive(Debug, Clone)]
pub struct ConsensusObservations {
    observations: Vec<(u64 /* generation */, Option<Duration>)>, // keyed by authority index
    stake_weighted_median: Duration,                             // cached value
}

impl ConsensusObservations {
    fn update_stake_weighted_median(&mut self, committee: &Committee) {
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

        let median_stake = stake_with_observations / 2;
        let mut running_stake = 0;
        for (duration, authority_index) in sorted_observations {
            running_stake += committee.stake_by_index(authority_index).unwrap();
            if running_stake > median_stake {
                self.stake_weighted_median = duration;
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
            Item = (AuthorityIndex, u64, ExecutionTimeObservationKey, Duration),
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
            observation.update_stake_weighted_median(&estimator.committee);
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
        generation: u64,
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
        generation: u64,
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
                    stake_weighted_median: Duration::ZERO,
                }
            });

        let (obs_generation, obs_duration) =
            &mut observations.observations[TryInto::<usize>::try_into(source).unwrap()];
        if *obs_generation >= generation {
            // Ignore outdated observation.
            return;
        }
        *obs_generation = generation;
        *obs_duration = Some(duration);
        if !skip_update {
            observations.update_stake_weighted_median(&self.committee);
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
                    .map(|obs| obs.stake_weighted_median)
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
    use sui_types::transaction::{Argument, CallArg, ObjectArg, ProgrammableMoveCall};

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
        observer.record_local_observations(&ptb, &timings, total_duration);

        let key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };

        // Check that local observation was recorded and shared
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.moving_average.get_average(),
            // 10ms overhead should be entirely apportioned to the one command in the PTB
            Duration::from_millis(110)
        );
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(110));

        // Record another observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(110))];
        let total_duration = Duration::from_millis(120);
        observer.record_local_observations(&ptb, &timings, total_duration);

        // Check that moving average was updated
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.moving_average.get_average(),
            // average of 110ms and 120ms observations
            Duration::from_millis(115)
        );
        // new 115ms average should not be shared; it's <5% different from 110ms
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(110));

        // Record another observation
        let timings = vec![ExecutionTiming::Success(Duration::from_millis(120))];
        let total_duration = Duration::from_millis(130);
        observer.record_local_observations(&ptb, &timings, total_duration);

        // Check that moving average was updated
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.moving_average.get_average(),
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
        let total_duration = Duration::from_millis(120);
        observer.record_local_observations(&ptb, &timings, total_duration);

        // Verify that moving average is the same and a new observation was shared, as
        // enough time has now elapsed
        let local_obs = observer.local_observations.get(&key).unwrap();
        assert_eq!(
            local_obs.moving_average.get_average(),
            // average of [110ms, 120ms, 120ms, 130ms]
            Duration::from_millis(120)
        );
        assert_eq!(local_obs.last_shared.unwrap().0, Duration::from_millis(120));
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
        observer.record_local_observations(&ptb, &timings, total_duration);

        // Check that both commands were recorded
        let move_key = ExecutionTimeObservationKey::MoveEntryPoint {
            package,
            module: module.clone(),
            function: function.clone(),
            type_arguments: vec![],
        };
        let move_obs = observer.local_observations.get(&move_key).unwrap();
        assert_eq!(
            move_obs.moving_average.get_average(),
            // 100/150 == 2/3 of 30ms overhead distributed to Move command
            Duration::from_millis(120)
        );

        let transfer_obs = observer
            .local_observations
            .get(&ExecutionTimeObservationKey::TransferObjects)
            .unwrap();
        assert_eq!(
            transfer_obs.moving_average.get_average(),
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
        );

        // Create a simple PTB with one move call and one mutable shared input
        let package = ObjectID::random();
        let module = "test_module".to_string();
        let function = "test_function".to_string();
        let ptb = ProgrammableTransaction {
            inputs: vec![CallArg::Object(ObjectArg::SharedObject {
                id: ObjectID::random(),
                initial_shared_version: SequenceNumber::new(),
                mutable: true,
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
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(2));
        assert!(observer
            .local_observations
            .get(&key)
            .unwrap()
            .last_shared
            .is_none());

        // Second observation - no time has passed, so now utilization is high; should share
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(1))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(2));
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

        // Third execution still with high utilization - time has passed but not enough to clear excess
        // when accounting for the new observation; should share
        tokio::time::advance(Duration::from_secs(5)).await;
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(3))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(5));
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

        // Fourth execution after utilization drops - should not share, even though diff still high
        tokio::time::advance(Duration::from_secs(60)).await;
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(11))];
        observer.record_local_observations(&ptb, &timings, Duration::from_secs(11));
        assert_eq!(
            observer
                .local_observations
                .get(&key)
                .unwrap()
                .last_shared
                .unwrap()
                .0,
            Duration::from_secs(3) // still the old value
        );
    }

    #[tokio::test]
    async fn test_stake_weighted_median() {
        telemetry_subscribers::init_for_testing();

        let (committee, _) =
            Committee::new_simple_test_committee_with_normalized_voting_power(vec![10, 20, 30, 40]);

        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, Some(Duration::from_secs(2))), // 20% stake
                (0, Some(Duration::from_secs(3))), // 30% stake
                (0, Some(Duration::from_secs(4))), // 40% stake
            ],
            stake_weighted_median: Duration::ZERO,
        };
        tracker.update_stake_weighted_median(&committee);
        // With stake weights [10,20,30,40]:
        // - Duration 1 covers 10% of stake
        // - Duration 2 covers 30% of stake (10+20)
        // - Duration 3 covers 60% of stake (10+20+30)
        // - Duration 4 covers 100% of stake
        // Median should be 3 since that's where we cross 50% of stake
        assert_eq!(tracker.stake_weighted_median, Duration::from_secs(3));

        // Test duration sorting
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(3))), // 10% stake
                (0, Some(Duration::from_secs(4))), // 20% stake
                (0, Some(Duration::from_secs(1))), // 30% stake
                (0, Some(Duration::from_secs(2))), // 40% stake
            ],
            stake_weighted_median: Duration::ZERO,
        };
        tracker.update_stake_weighted_median(&committee);
        // With sorted stake weights [30,40,10,20]:
        // - Duration 1 covers 30% of stake
        // - Duration 2 covers 70% of stake (30+40)
        // - Duration 3 covers 80% of stake (30+40+10)
        // - Duration 4 covers 100% of stake
        // Median should be 2 since that's where we cross 50% of stake
        assert_eq!(tracker.stake_weighted_median, Duration::from_secs(2));

        // Test with one missing observation
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, None),                         // 20% stake (missing)
                (0, Some(Duration::from_secs(3))), // 30% stake
                (0, Some(Duration::from_secs(4))), // 40% stake
            ],
            stake_weighted_median: Duration::ZERO,
        };
        tracker.update_stake_weighted_median(&committee);
        // With missing observation for 20% stake:
        // - Duration 1 covers 10% of stake
        // - Duration 3 covers 40% of stake (10+30)
        // - Duration 4 covers 80% of stake (10+30+40)
        // Median should be 4 since that's where we pass half of available stake (80% / 2 == 40%)
        assert_eq!(tracker.stake_weighted_median, Duration::from_secs(4));

        // Test with multiple missing observations
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(1))), // 10% stake
                (0, Some(Duration::from_secs(2))), // 20% stake
                (0, None),                         // 30% stake (missing)
                (0, None),                         // 40% stake (missing)
            ],
            stake_weighted_median: Duration::ZERO,
        };
        tracker.update_stake_weighted_median(&committee);
        // With missing observations:
        // - Duration 1 covers 10% of stake
        // - Duration 2 covers 30% of stake (10+20)
        // Median should be 2 since that's where we cross half of available stake (40% / 2 == 20%)
        assert_eq!(tracker.stake_weighted_median, Duration::from_secs(2));

        // Test with one observation
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, None),                         // 10% stake
                (0, None),                         // 20% stake
                (0, Some(Duration::from_secs(3))), // 30% stake
                (0, None),                         // 40% stake
            ],
            stake_weighted_median: Duration::ZERO,
        };
        tracker.update_stake_weighted_median(&committee);
        // With only one observation, median should be that observation
        assert_eq!(tracker.stake_weighted_median, Duration::from_secs(3));

        // Test with all same durations
        let mut tracker = ConsensusObservations {
            observations: vec![
                (0, Some(Duration::from_secs(5))), // 10% stake
                (0, Some(Duration::from_secs(5))), // 20% stake
                (0, Some(Duration::from_secs(5))), // 30% stake
                (0, Some(Duration::from_secs(5))), // 40% stake
            ],
            stake_weighted_median: Duration::ZERO,
        };
        tracker.update_stake_weighted_median(&committee);
        assert_eq!(tracker.stake_weighted_median, Duration::from_secs(5));
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
            1,
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            1,
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            1,
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );

        estimator.process_observation_from_consensus(
            0,
            1,
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            1,
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            1,
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );

        // Now record newer observations that should be used
        estimator.process_observation_from_consensus(
            0,
            2,
            move_key.clone(),
            Duration::from_millis(100),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            2,
            move_key.clone(),
            Duration::from_millis(200),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            2,
            move_key.clone(),
            Duration::from_millis(300),
            false,
        );

        estimator.process_observation_from_consensus(
            0,
            2,
            transfer_key.clone(),
            Duration::from_millis(50),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            2,
            transfer_key.clone(),
            Duration::from_millis(60),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            2,
            transfer_key.clone(),
            Duration::from_millis(70),
            false,
        );

        // Try to record old observations again - these should be ignored
        estimator.process_observation_from_consensus(
            0,
            1,
            move_key.clone(),
            Duration::from_millis(1000),
            false,
        );
        estimator.process_observation_from_consensus(
            1,
            1,
            transfer_key.clone(),
            Duration::from_millis(500),
            false,
        );
        estimator.process_observation_from_consensus(
            2,
            1,
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
}
