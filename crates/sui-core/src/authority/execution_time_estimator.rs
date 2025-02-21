// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use super::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::consensus_adapter::SubmitToConsensus;
use itertools::Itertools;
use lru::LruCache;
use mysten_common::debug_fatal;
use mysten_metrics::{monitored_scope, spawn_monitored_task};
use simple_moving_average::{SingleSumSMA, SMA};
use sui_protocol_config::PerObjectCongestionControlMode;
use sui_types::{
    committee::Committee,
    error::SuiError,
    execution::{ExecutionTimeObservationKey, ExecutionTiming},
    messages_consensus::{AuthorityIndex, ConsensusTransaction, ExecutionTimeObservation},
    transaction::{
        Command, ProgrammableTransaction, StoredExecutionTimeObservations, TransactionData,
        TransactionDataAPI, TransactionKind,
    },
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

const LOCAL_OBSERVATION_WINDOW_SIZE: usize = 10;

// If our current local observation differs from the last one we shared by more than
// this percent, we share a new one.
const OBSERVATION_SHARING_DIFF_THRESHOLD: f64 = 0.05;

// Minimum interval between sharing multiple observations of the same key.
const OBSERVATION_SHARING_MIN_INTERVAL: Duration = Duration::from_secs(5);

// Collects local execution time estimates to share via consensus.
pub struct ExecutionTimeObserver {
    epoch_store: Weak<AuthorityPerEpochStore>,
    consensus_adapter: Box<dyn SubmitToConsensus>,

    local_observations: LruCache<ExecutionTimeObservationKey, LocalObservations>,
}

#[derive(Debug, Clone)]
pub struct LocalObservations {
    moving_average: SingleSumSMA<Duration, u32, LOCAL_OBSERVATION_WINDOW_SIZE>,
    last_shared: Option<(Duration, Instant)>,
}

// Tracks local execution time observations and shares them via consensus.
impl ExecutionTimeObserver {
    pub fn spawn(
        epoch_store: Arc<AuthorityPerEpochStore>,
        consensus_adapter: Box<dyn SubmitToConsensus>,
        channel_size: usize,
        lru_cache_size: NonZeroUsize,
    ) {
        if epoch_store
            .protocol_config()
            .per_object_congestion_control_mode()
            != PerObjectCongestionControlMode::ExecutionTimeEstimate
        {
            info!("ExecutionTimeObserver disabled because per-object congestion control mode is not ExecutionTimeEstimate");
            return;
        }

        let (tx_local_execution_time, mut rx_local_execution_time) = mpsc::channel(channel_size);
        epoch_store.set_local_execution_time_channel(tx_local_execution_time);

        // TODO: pre-populate local observations with stored data from prior epoch.
        let mut observer = Self {
            epoch_store: Arc::downgrade(&epoch_store),
            consensus_adapter,
            local_observations: LruCache::new(lru_cache_size),
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
    ) -> Self {
        Self {
            epoch_store: Arc::downgrade(&epoch_store),
            consensus_adapter,
            local_observations: LruCache::new(NonZeroUsize::new(10000).unwrap()),
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

            // Send a new observation through consensus if our current moving average
            // differs too much from the last one we shared.
            // TODO: Consider only sharing observations for entrypoints with congestion.
            // TODO: Consider only sharing observations that disagree with consensus estimate.
            let new_average = local_observation.moving_average.get_average();
            if local_observation
                .last_shared
                .map_or(true, |(last_shared, last_shared_timestamp)| {
                    let diff = last_shared.abs_diff(new_average);
                    diff > new_average.mul_f64(OBSERVATION_SHARING_DIFF_THRESHOLD)
                        && last_shared_timestamp.elapsed() > OBSERVATION_SHARING_MIN_INTERVAL
                })
            {
                debug!("sharing new execution time observation for {key:?}: {new_average:?}");
                to_share.push((key, new_average));
                local_observation.last_shared = Some((new_average, Instant::now()));
            }
        }

        // Share new observations.
        if !to_share.is_empty() {
            if let Some(epoch_store) = self.epoch_store.upgrade() {
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
                        warn!("failed to submit execution time observation: {e:?}");
                    }
                }
            }
        }
    }
}

// Default duration estimate used for transations containing a command without any
// available observations.
// TODO: Make this a protocol config.
const DEFAULT_TRANSACTION_DURATION: Duration = Duration::from_millis(1_500);

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
        let mut estimate = Duration::ZERO;
        for command in &tx.commands {
            let key = ExecutionTimeObservationKey::from_command(command);
            let Some(command_estimate) = self.consensus_observations.get(&key).map(|obs| {
                obs.stake_weighted_median
                    .mul_f64(command_length(command).get() as f64)
            }) else {
                estimate = DEFAULT_TRANSACTION_DURATION;
                break;
            };
            estimate += command_estimate;
        }
        estimate
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
    use sui_types::base_types::{ObjectID, SuiAddress};
    use sui_types::transaction::{Argument, ProgrammableMoveCall};

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
