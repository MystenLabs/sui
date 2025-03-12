// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroUsize},
    sync::{Arc, Weak},
    time::Duration,
};

use super::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::SubmitToConsensus;
use governor::{Quota, RateLimiter};
use itertools::Itertools;
use lru::LruCache;
use mysten_common::debug_fatal;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use nonzero_ext::nonzero;
use simple_moving_average::{SingleSumSMA, SMA};
use sui_protocol_config::PerObjectCongestionControlMode;
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

// TODO: Move all these consts into protocol configs once design stabilizes.

const MAX_ESTIMATED_TRANSACTION_DURATION: Duration = Duration::from_millis(1_500);

const LOCAL_OBSERVATION_WINDOW_SIZE: usize = 10;

// We won't share a new observation with consensus unless our current local observation differs
// from the last one we shared by more than this percentage.
const OBSERVATION_SHARING_DIFF_THRESHOLD: f64 = 0.05;

// We won't share a new observation with consensus unless target object utilization is exceeded
// by at least this amount.
const OBSERVATION_SHARING_OBJECT_UTILIZATION_THRESHOLD: Duration = Duration::from_millis(500);

// Minimum interval between sharing multiple observations of the same key.
const OBSERVATION_SHARING_MIN_INTERVAL: Duration = Duration::from_secs(5);

// Global rate limit for sharing observations. This is a safety valve and should
// not trigger during normal operation.
const OBSERVATION_SHARING_RATE_LIMIT: NonZeroU32 = nonzero!(10u32); // per second
const OBSERVATION_SHARING_BURST_LIMIT: NonZeroU32 = nonzero!(60u32);

const OBJECT_UTILIZATION_TRACKER_CAPACITY: usize = 50_000;

// TODO: source from time-based utilization target param in ProtocolConfig when available.
const TARGET_OBJECT_UTILIZATION: f64 = 0.5;

// Collects local execution time estimates to share via consensus.
pub struct ExecutionTimeObserver {
    epoch_store: Weak<AuthorityPerEpochStore>,
    consensus_adapter: Box<dyn SubmitToConsensus>,
    observation_sharing_object_utilization_threshold: Duration,

    local_observations: LruCache<ExecutionTimeObservationKey, LocalObservations>,

    // For each object, tracks the amount of time above our utilization target that we spent
    // executing transactions. This is used to decide which observations should be shared
    // via consensus.
    object_utilization_tracker: LruCache<ObjectID, ObjectUtilization>,

    sharing_rate_limiter: RateLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::DefaultClock,
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
}

// Tracks local execution time observations and shares them via consensus.
impl ExecutionTimeObserver {
    pub fn spawn(
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_adapter: Box<dyn SubmitToConsensus>,
        channel_size: usize,
        observation_cache_size: NonZeroUsize,
    ) {
        if !matches!(
            epoch_store
                .protocol_config()
                .per_object_congestion_control_mode(),
            PerObjectCongestionControlMode::ExecutionTimeEstimate(_)
        ) {
            info!("ExecutionTimeObserver disabled because per-object congestion control mode is not ExecutionTimeEstimate");
            return;
        }

        let (tx_local_execution_time, mut rx_local_execution_time) = mpsc::channel(channel_size);
        epoch_store.set_local_execution_time_channel(tx_local_execution_time);

        // TODO: pre-populate local observations with stored data from prior epoch.
        let mut observer = Self {
            epoch_store: Arc::downgrade(&epoch_store),
            consensus_adapter,
            local_observations: LruCache::new(observation_cache_size),
            object_utilization_tracker: LruCache::new(
                NonZeroUsize::new(OBJECT_UTILIZATION_TRACKER_CAPACITY).unwrap(),
            ),
            observation_sharing_object_utilization_threshold:
                OBSERVATION_SHARING_OBJECT_UTILIZATION_THRESHOLD,
            sharing_rate_limiter: RateLimiter::direct(
                Quota::per_second(OBSERVATION_SHARING_RATE_LIMIT)
                    .allow_burst(OBSERVATION_SHARING_BURST_LIMIT),
            ),
        };
        spawn_monitored_task!(epoch_store.within_alive_epoch(async move {
            while let Some((tx, timings, total_duration)) = rx_local_execution_time.recv().await {
                observer
                    .record_local_observations(&tx, &timings, total_duration)
                    .await;
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
        Self {
            epoch_store: Arc::downgrade(&epoch_store),
            consensus_adapter,
            local_observations: LruCache::new(NonZeroUsize::new(10000).unwrap()),
            object_utilization_tracker: LruCache::new(
                NonZeroUsize::new(OBJECT_UTILIZATION_TRACKER_CAPACITY).unwrap(),
            ),
            observation_sharing_object_utilization_threshold,
            sharing_rate_limiter: RateLimiter::direct(Quota::per_hour(NonZeroU32::MAX)),
        }
    }

    // Used by execution to report observed per-entry-point execution times to the estimator.
    // Updates moving averages and submits observation to consensus if local observation differs
    // from consensus median.
    // TODO: Consider more detailed heuristic to account for overhead outside of commands.
    async fn record_local_observations(
        &mut self,
        tx: &ProgrammableTransaction,
        timings: &[ExecutionTiming],
        total_duration: Duration,
    ) {
        let _scope = monitored_scope("ExecutionTimeObserver::record_local_observations");

        assert!(tx.commands.len() >= timings.len());

        // Update the accumulated excess execution time for each mutable shared object
        // used in this transaction, and determine the max overage.
        let max_excess_per_object_execution_time = tx
            .shared_input_objects()
            .filter_map(|obj| obj.mutable.then_some(obj.id))
            .map(|id| {
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
                        });
                utilization.excess_execution_time += total_duration;
                utilization.excess_execution_time =
                    utilization.excess_execution_time.saturating_sub(
                        utilization
                            .last_measured
                            .map(|last_measured| now.duration_since(last_measured))
                            .unwrap_or(Duration::MAX)
                            .mul_f64(TARGET_OBJECT_UTILIZATION),
                    );
                utilization.last_measured = Some(now);
                utilization.excess_execution_time
            })
            .max()
            .unwrap_or(Duration::ZERO);

        let total_command_duration: Duration = timings.iter().map(|t| t.duration()).sum();
        let extra_overhead = total_duration - total_command_duration;

        let mut to_share = Vec::with_capacity(tx.commands.len());
        for (i, timing) in timings.iter().enumerate() {
            let command = &tx.commands[i];
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
                        diff >= new_average.mul_f64(OBSERVATION_SHARING_DIFF_THRESHOLD)
                            && last_shared_timestamp.elapsed() >= OBSERVATION_SHARING_MIN_INTERVAL
                    });
            let utilization_exceeds_threshold = max_excess_per_object_execution_time
                >= self.observation_sharing_object_utilization_threshold;
            if diff_exceeds_threshold && utilization_exceeds_threshold {
                debug!("sharing new execution time observation for {key:?}: {new_average:?}");
                to_share.push((key, new_average));
                local_observation.last_shared = Some((new_average, Instant::now()));
            }
        }

        // Share new observations.
        self.share_observations(to_share).await;
    }

    async fn share_observations(&mut self, to_share: Vec<(ExecutionTimeObservationKey, Duration)>) {
        if to_share.is_empty() {
            return;
        }
        let Some(epoch_store) = self.epoch_store.upgrade() else {
            debug!("epoch is ending, dropping execution time observation");
            return;
        };

        // Enforce global observation-sharing rate limit.
        if let Err(e) = self.sharing_rate_limiter.check() {
            debug!("rate limit exceeded, dropping execution time observation; {e:?}");
            // TODO: Increment a metric for dropped observations, for alerting.
            return;
        }

        let epoch_store = epoch_store.clone();
        epoch_store
            .metrics
            .epoch_execution_time_observations_shared
            .inc();
        let transaction = ConsensusTransaction::new_execution_time_observation(
            ExecutionTimeObservation::new(epoch_store.name, to_share),
        );
        if let Err(e) = self
            .consensus_adapter
            .submit_to_consensus(&[transaction], &epoch_store)
        {
            if !matches!(e, SuiError::EpochEnded(_)) {
                // TODO: Increment a metric for dropped observations, for alerting.
                warn!("failed to submit execution time observation: {e:?}");
            }
        }
    }
}

// Key used to save StoredExecutionTimeObservations in the Sui system state object's
// `extra_fields` Bag.
pub const EXTRA_FIELD_EXECUTION_TIME_ESTIMATES_KEY: u64 = 0;

// Tracks global execution time observations provided by validators from consensus
// and computes deterministic per-command estimates for use in congestion control.
pub struct ExecutionTimeEstimator {
    committee: Arc<Committee>,

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
        initial_observations: impl Iterator<
            Item = (AuthorityIndex, u64, ExecutionTimeObservationKey, Duration),
        >,
    ) -> Self {
        // TODO: At epoch start, prepopulate with end-of-epoch data from previous epoch.
        // TODO: Load saved consensus observations from per-epoch tables for crash recovery.
        let mut estimator = Self {
            committee,
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
            .min(MAX_ESTIMATED_TRANSACTION_DURATION)
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
    use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
    use sui_types::transaction::{Argument, CallArg, ObjectArg, ProgrammableMoveCall};

    #[tokio::test]
    async fn test_record_local_observations() {
        telemetry_subscribers::init_for_testing();

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
        observer
            .record_local_observations(&ptb, &timings, total_duration)
            .await;

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
        observer
            .record_local_observations(&ptb, &timings, total_duration)
            .await;

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
        observer
            .record_local_observations(&ptb, &timings, total_duration)
            .await;

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
        observer
            .record_local_observations(&ptb, &timings, total_duration)
            .await;

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
        observer
            .record_local_observations(&ptb, &timings, total_duration)
            .await;

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
        observer
            .record_local_observations(&ptb, &timings, Duration::from_secs(2))
            .await;
        assert!(observer
            .local_observations
            .get(&key)
            .unwrap()
            .last_shared
            .is_none());

        // Second observation - no time has passed, so now utilization is high; should share
        let timings = vec![ExecutionTiming::Success(Duration::from_secs(1))];
        observer
            .record_local_observations(&ptb, &timings, Duration::from_secs(2))
            .await;
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
        observer
            .record_local_observations(&ptb, &timings, Duration::from_secs(5))
            .await;
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
        observer
            .record_local_observations(&ptb, &timings, Duration::from_secs(11))
            .await;
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
        let mut estimator = ExecutionTimeEstimator::new(Arc::new(committee), std::iter::empty());
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
