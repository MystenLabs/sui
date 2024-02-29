// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::block::{Slot, GENESIS_ROUND};
use crate::context::Context;
use crate::core::{CoreSignalsReceivers, QuorumUpdate, DEFAULT_NUM_LEADERS_PER_ROUND};
use crate::core_thread::CoreThreadDispatcher;
use std::cmp::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{sleep_until, Instant};
use tracing::{debug, warn};

#[allow(unused)]
const TOTAL_LEADER_TIMEOUT_WEIGHT: u32 = 100;

/// The leader timeout weights used to update the remaining timeout according to each leader weight.
/// Each position on the array represents the weight of the leader of a round according to their ordered position.
/// For example, on an array with values [50, 30, 20], it means that:
/// * the first leader of the round has weight 50
/// * the second leader of the round has weight 30
/// * the third leader of the round has weight 20
///
/// The weights basically dictate by what fraction the total leader timeout should be reduced when a leader
/// is found for the round. For the reduction to happen each time it is important for the leader of the previous
/// position to have been found first. The rational is to reduce the total waiting time to timeout/propose every time
/// that we have successfully received a leader in order.
#[allow(unused)]
pub(crate) const DEFAULT_LEADER_TIMEOUT_WEIGHTS: [u32; DEFAULT_NUM_LEADERS_PER_ROUND] =
    [TOTAL_LEADER_TIMEOUT_WEIGHT];

pub(crate) struct LeaderTimeoutTaskHandle {
    handle: JoinHandle<()>,
    stop: Sender<()>,
}

impl LeaderTimeoutTaskHandle {
    pub async fn stop(self) {
        self.stop.send(()).ok();
        self.handle.await.ok();
    }
}

pub(crate) struct LeaderTimeoutTask<
    D: CoreThreadDispatcher,
    const NUM_OF_LEADERS: usize = DEFAULT_NUM_LEADERS_PER_ROUND,
> {
    dispatcher: Arc<D>,
    quorum_update_receiver: watch::Receiver<QuorumUpdate>,
    stop: Receiver<()>,
    leader_timeout: Duration,
    leader_timeout_weights: [u32; NUM_OF_LEADERS],
}

impl<D: CoreThreadDispatcher, const NUM_OF_LEADERS: usize> LeaderTimeoutTask<D, NUM_OF_LEADERS> {
    pub fn start(
        dispatcher: Arc<D>,
        signals_receivers: &CoreSignalsReceivers,
        context: Arc<Context>,
        leader_timeout_weights: [u32; NUM_OF_LEADERS],
    ) -> LeaderTimeoutTaskHandle {
        assert_timeout_weights(&leader_timeout_weights);

        let (stop_sender, stop) = tokio::sync::oneshot::channel();
        let mut me = Self {
            dispatcher,
            stop,
            quorum_update_receiver: signals_receivers.quorum_update_receiver(),
            leader_timeout: context.parameters.leader_timeout,
            leader_timeout_weights,
        };
        let handle = tokio::spawn(async move { me.run().await });

        LeaderTimeoutTaskHandle {
            handle,
            stop: stop_sender,
        }
    }

    async fn run(&mut self) {
        let mut last_quorum_update: QuorumUpdate = QuorumUpdate::default();

        let mut leader_round_timed_out = false;
        let mut last_quorum_time = Instant::now();
        let leader_timeout = sleep_until(last_quorum_time + self.leader_timeout);

        tokio::pin!(leader_timeout);

        loop {
            tokio::select! {
                // when leader timer expires then we attempt to trigger the creation of a new block.
                // If we already timed out before then the branch gets disabled, so we don't attempt
                // all the time to produce already produced blocks for that round.
                () = &mut leader_timeout, if !leader_round_timed_out => {
                    if let Err(err) = self.dispatcher.force_new_block(last_quorum_update.round.saturating_add(1)).await {
                        warn!("Error received while calling dispatcher, probably dispatcher is shutting down, will now exit: {err:?}");
                        return;
                    }
                    leader_round_timed_out = true;
                }

                // Either a new quorum round has been produced or new leaders have been accepted. Reset the leader timeout.
                Ok(_) = self.quorum_update_receiver.changed() => {
                    let update: QuorumUpdate = self.quorum_update_receiver.borrow_and_update().clone();

                    if update.round > GENESIS_ROUND {
                        assert_eq!(update.leaders.len(), NUM_OF_LEADERS, "Number of expected leaders differ from the leader timeout weight setup");
                    }

                    match update.round.cmp(&last_quorum_update.round) {
                        Ordering::Less => {
                            warn!("Received leader update for lower quorum round {} compared to previous round {}, will ignore", update.round, last_quorum_update.round);
                            continue;
                        }
                        Ordering::Equal => {
                            // if we have already timed out and keep receiving updates for the same
                            // round or nothing changed on the updated leaders, just continue
                            if leader_round_timed_out || update.leaders.eq(&last_quorum_update.leaders) {
                                continue;
                            }

                            leader_timeout
                            .as_mut()
                            .reset(last_quorum_time + self.calculate_leader_timeout(&update.leaders));
                        }
                        Ordering::Greater => {
                            debug!("New round has been received {}, resetting timer", update.round);

                            leader_round_timed_out = false;
                            last_quorum_time = Instant::now();

                            leader_timeout
                            .as_mut()
                            .reset(last_quorum_time + self.calculate_leader_timeout(&update.leaders));
                        }
                    }

                    last_quorum_update = update;
                },
                _ = &mut self.stop => {
                    debug!("Stop signal has been received, now shutting down");
                    return;
                }
            }
        }
    }

    // Calculates the leader(s) timeout. The timeout is calculated based on the number of total
    // expected leaders and the configured timeout weights.
    fn calculate_leader_timeout(&self, leaders: &[Option<Slot>]) -> Duration {
        // The most important leader is located in position 0 for the `leaders` array.
        // The least important is last. We want to sum the weight only based
        // on the found leaders. Once a `None` is found - meaning a leader is missing for that slot -
        // we want to abort as we don't want to expense its position from waiting.
        let mut weight = 0;
        for (i, leader) in leaders.iter().enumerate() {
            if leader.is_some() {
                weight += self.leader_timeout_weights[i];
            } else {
                break;
            }
        }

        // Now calculate the updated timeout time
        self.leader_timeout - (weight * self.leader_timeout) / TOTAL_LEADER_TIMEOUT_WEIGHT
    }
}

fn assert_timeout_weights(weights: &[u32]) {
    let mut total = 0;
    for w in weights {
        total += w;
    }
    assert_eq!(
        total, TOTAL_LEADER_TIMEOUT_WEIGHT,
        "Total weight should be 100"
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use consensus_config::{AuthorityIndex, Parameters};
    use parking_lot::Mutex;
    use tokio::sync::watch;
    use tokio::time::{sleep, Instant};

    use crate::block::{BlockRef, Round, Slot, VerifiedBlock};
    use crate::context::Context;
    use crate::core::{CoreSignals, QuorumUpdate, DEFAULT_NUM_LEADERS_PER_ROUND};
    use crate::core_thread::{CoreError, CoreThreadDispatcher};
    use crate::leader_timeout::{LeaderTimeoutTask, DEFAULT_LEADER_TIMEOUT_WEIGHTS};

    #[derive(Clone, Default)]
    struct MockCoreThreadDispatcher {
        force_new_block_calls: Arc<Mutex<Vec<(Round, Instant)>>>,
    }

    impl MockCoreThreadDispatcher {
        async fn get_force_new_block_calls(&self) -> Vec<(Round, Instant)> {
            let mut binding = self.force_new_block_calls.lock();
            let all_calls = binding.drain(0..);
            all_calls.into_iter().collect()
        }
    }

    #[async_trait]
    impl CoreThreadDispatcher for MockCoreThreadDispatcher {
        async fn add_blocks(
            &self,
            _blocks: Vec<VerifiedBlock>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            todo!()
        }

        async fn force_new_block(&self, round: Round) -> Result<(), CoreError> {
            self.force_new_block_calls
                .lock()
                .push((round, Instant::now()));
            Ok(())
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            todo!()
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn basic_leader_timeout() {
        let (context, _signers) = Context::new_for_test(4);
        let dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let leader_timeout = Duration::from_millis(500);
        let parameters = Parameters {
            leader_timeout,
            ..Default::default()
        };
        let context = Arc::new(context.with_parameters(parameters));
        let start = Instant::now();

        let (mut signals, signal_receivers) = CoreSignals::new(context.clone());

        // spawn the task
        let _handle = LeaderTimeoutTask::start(
            dispatcher.clone(),
            &signal_receivers,
            context,
            DEFAULT_LEADER_TIMEOUT_WEIGHTS,
        );

        // send a signal that a new round has been produced.
        signals
            .quorum_update(QuorumUpdate::new(
                9,
                vec![None; DEFAULT_NUM_LEADERS_PER_ROUND],
            ))
            .ok();

        // wait enough until a force_new_block has been received
        sleep(2 * leader_timeout).await;
        let all_calls = dispatcher.get_force_new_block_calls().await;

        assert_eq!(all_calls.len(), 1);

        let (round, timestamp) = all_calls[0];
        assert_eq!(round, 10);
        assert!(
            leader_timeout <= timestamp - start,
            "Leader timeout setting {:?} should be less than actual time difference {:?}",
            leader_timeout,
            timestamp - start
        );

        // now wait another 2 * leader_timeout, no other call should be received
        sleep(2 * leader_timeout).await;
        let all_calls = dispatcher.get_force_new_block_calls().await;

        assert_eq!(all_calls.len(), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn multiple_leader_timeouts() {
        let (context, _signers) = Context::new_for_test(4);
        let dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let leader_timeout = Duration::from_millis(500);
        let parameters = Parameters {
            leader_timeout,
            ..Default::default()
        };
        let context = Arc::new(context.with_parameters(parameters));
        let now = Instant::now();

        let (mut signals, signal_receivers) = CoreSignals::new(context.clone());

        // spawn the task
        let _handle = LeaderTimeoutTask::start(
            dispatcher.clone(),
            &signal_receivers,
            context,
            DEFAULT_LEADER_TIMEOUT_WEIGHTS,
        );

        // now send some signals with some small delay between them, but not enough so every round
        // manages to timeout and call the force new block method.
        signals
            .quorum_update(QuorumUpdate::new(
                12,
                vec![None; DEFAULT_NUM_LEADERS_PER_ROUND],
            ))
            .ok();
        sleep(leader_timeout / 2).await;
        signals
            .quorum_update(QuorumUpdate::new(
                13,
                vec![None; DEFAULT_NUM_LEADERS_PER_ROUND],
            ))
            .ok();
        sleep(leader_timeout / 2).await;
        signals
            .quorum_update(QuorumUpdate::new(
                14,
                vec![None; DEFAULT_NUM_LEADERS_PER_ROUND],
            ))
            .ok();
        sleep(2 * leader_timeout).await;

        // only the last one should be received
        let all_calls = dispatcher.get_force_new_block_calls().await;
        let (round, timestamp) = all_calls[0];
        assert_eq!(round, 15);
        assert!(leader_timeout < timestamp - now);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn multiple_quorum_updates_for_same_round() {
        const NUM_OF_LEADERS_PER_ROUND: usize = 3;
        let (context, _signers) = Context::new_for_test(4);
        let dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let leader_timeout = Duration::from_millis(500);
        let parameters = Parameters {
            leader_timeout,
            ..Default::default()
        };
        let context = Arc::new(context.with_parameters(parameters));
        let now = Instant::now();

        let (mut signals, signal_receivers) = CoreSignals::new(context.clone());

        // We expected 3 leaders. For each leader we set the weight.
        let timeout_weights = [40, 30, 30];

        // spawn the task
        let _handle = LeaderTimeoutTask::start(
            dispatcher.clone(),
            &signal_receivers,
            context,
            timeout_weights,
        );

        // Send a quorum update for round 12 and without leaders found. This will reset the timer and
        // adjust in order to timeout with maximum default value - 500ms
        signals
            .quorum_update(QuorumUpdate::new(12, vec![None; NUM_OF_LEADERS_PER_ROUND]))
            .ok();
        sleep(Duration::from_millis(100)).await;

        // Now send an update for a leader found on the second position, nothing should change on the
        // expected leader timeout as we still miss a more important leader on the left - the one from first position.
        // So we want to wait for the leader of position one before we adjust the timer.
        signals
            .quorum_update(QuorumUpdate::new(
                12,
                vec![
                    None,
                    Some(Slot::new(12, AuthorityIndex::new_for_test(2))),
                    None,
                ],
            ))
            .ok();
        sleep(Duration::from_millis(100)).await;

        // Now send an update that we have found a leader in first position. So we now have found the leaders
        // on the first and second position. The total leader timeout should be reduced according to the weights by:
        // 1) 40 * 500 / 100 = 200ms
        // 2) 30 * 50 / 100 = 150ms
        // So in total the timeout should be reduced from the initial 500ms to 500ms - 200ms - 150ms = 150ms.
        // The timeout should be reset to ensure that for the round 12 we will (or have already) waited at most 150ms
        // before attempting for produce a new block.
        signals
            .quorum_update(QuorumUpdate::new(
                12,
                vec![
                    Some(Slot::new(12, AuthorityIndex::new_for_test(3))),
                    Some(Slot::new(12, AuthorityIndex::new_for_test(2))),
                    None,
                ],
            ))
            .ok();

        // Give a little bit of time to make sure that the new block call has run
        sleep(Duration::from_millis(50)).await;

        // only the last one should be received
        let all_calls = dispatcher.get_force_new_block_calls().await;
        let (round, timestamp) = all_calls[0];
        assert_eq!(round, 13);
        assert!(timestamp - now < Duration::from_millis(250));
    }

    #[test]
    fn leader_timeout_calculation() {
        let dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let (_quorum_update_sender, quorum_update_receiver) =
            watch::channel(QuorumUpdate::default());
        let (_stop_sender, stop) = tokio::sync::oneshot::channel();

        // For 3 leaders
        let leader_timeout_weights = [50, 40, 10];

        let task = LeaderTimeoutTask {
            dispatcher,
            quorum_update_receiver,
            stop,
            leader_timeout: Duration::from_millis(500),
            leader_timeout_weights,
        };

        // WHEN all are None
        let leaders = [None, None, None];
        let timeout = task.calculate_leader_timeout(&leaders);

        assert_eq!(timeout, Duration::from_millis(500));

        // WHEN third leader is Some
        let leaders = [
            None,
            None,
            Some(Slot::new(5, AuthorityIndex::new_for_test(2))),
        ];
        let timeout = task.calculate_leader_timeout(&leaders);

        assert_eq!(timeout, Duration::from_millis(500));

        // WHEN second & third leader is Some
        let leaders = [
            None,
            Some(Slot::new(5, AuthorityIndex::new_for_test(1))),
            Some(Slot::new(5, AuthorityIndex::new_for_test(2))),
        ];
        let timeout = task.calculate_leader_timeout(&leaders);

        assert_eq!(timeout, Duration::from_millis(500));

        // WHEN first & second leader is Some
        let leaders = [
            Some(Slot::new(5, AuthorityIndex::new_for_test(0))),
            Some(Slot::new(5, AuthorityIndex::new_for_test(1))),
            None,
        ];
        let timeout = task.calculate_leader_timeout(&leaders);

        assert_eq!(timeout, Duration::from_millis(50));
    }
}
