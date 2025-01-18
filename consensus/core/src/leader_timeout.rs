// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::block::Round;
use crate::context::Context;
use crate::core::CoreSignalsReceivers;
use crate::core_thread::CoreThreadDispatcher;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{sleep_until, Instant};
use tracing::{debug, warn};

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

pub(crate) struct LeaderTimeoutTask<D: CoreThreadDispatcher> {
    dispatcher: Arc<D>,
    new_round_receiver: watch::Receiver<Round>,
    leader_timeout: Duration,
    min_round_delay: Duration,
    stop: Receiver<()>,
}

impl<D: CoreThreadDispatcher> LeaderTimeoutTask<D> {
    pub fn start(
        dispatcher: Arc<D>,
        signals_receivers: &CoreSignalsReceivers,
        context: Arc<Context>,
    ) -> LeaderTimeoutTaskHandle {
        let (stop_sender, stop) = tokio::sync::oneshot::channel();
        let mut me = Self {
            dispatcher,
            stop,
            new_round_receiver: signals_receivers.new_round_receiver(),
            leader_timeout: context.parameters.leader_timeout,
            min_round_delay: context.parameters.min_round_delay,
        };
        let handle = tokio::spawn(async move { me.run().await });

        LeaderTimeoutTaskHandle {
            handle,
            stop: stop_sender,
        }
    }

    async fn run(&mut self) {
        let new_round = &mut self.new_round_receiver;
        let mut leader_round: Round = *new_round.borrow_and_update();
        let mut min_leader_round_timed_out = false;
        let mut max_leader_round_timed_out = false;
        let timer_start = Instant::now();
        let min_leader_timeout = sleep_until(timer_start + self.min_round_delay);
        let max_leader_timeout = sleep_until(timer_start + self.leader_timeout);

        tokio::pin!(min_leader_timeout);
        tokio::pin!(max_leader_timeout);

        loop {
            tokio::select! {
                // when the min leader timer expires then we attempt to trigger the creation of a new block.
                // If we already timed out before then the branch gets disabled so we don't attempt
                // all the time to produce already produced blocks for that round.
                () = &mut min_leader_timeout, if !min_leader_round_timed_out => {
                    if let Err(err) = self.dispatcher.new_block(leader_round, false).await {
                        warn!("Error received while calling dispatcher, probably dispatcher is shutting down, will now exit: {err:?}");
                        return;
                    }
                    min_leader_round_timed_out = true;
                },
                // When the max leader timer expires then we attempt to trigger the creation of a new block. This
                // call is made with `force = true` to bypass any checks that allow to propose immediately if block
                // not already produced.
                // Keep in mind that first the min timeout should get triggered and then the max timeout, only
                // if the round has not advanced in the meantime. Otherwise, the max timeout will not get
                // triggered at all.
                () = &mut max_leader_timeout, if !max_leader_round_timed_out => {
                    if let Err(err) = self.dispatcher.new_block(leader_round, true).await {
                        warn!("Error received while calling dispatcher, probably dispatcher is shutting down, will now exit: {err:?}");
                        return;
                    }
                    max_leader_round_timed_out = true;
                }

                // a new round has been produced. Reset the leader timeout.
                Ok(_) = new_round.changed() => {
                    leader_round = *new_round.borrow_and_update();
                    debug!("New round has been received {leader_round}, resetting timer");

                    min_leader_round_timed_out = false;
                    max_leader_round_timed_out = false;

                    let now = Instant::now();
                    min_leader_timeout
                    .as_mut()
                    .reset(now + self.min_round_delay);
                    max_leader_timeout
                    .as_mut()
                    .reset(now + self.leader_timeout);
                },
                _ = &mut self.stop => {
                    debug!("Stop signal has been received, now shutting down");
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use consensus_config::Parameters;
    use parking_lot::Mutex;
    use tokio::time::{sleep, Instant};

    use crate::block::{BlockRef, Round, VerifiedBlock};
    use crate::context::Context;
    use crate::core::CoreSignals;
    use crate::core_thread::{CoreError, CoreThreadDispatcher};
    use crate::leader_timeout::LeaderTimeoutTask;
    use crate::round_prober::QuorumRound;

    #[derive(Clone, Default)]
    struct MockCoreThreadDispatcher {
        new_block_calls: Arc<Mutex<Vec<(Round, bool, Instant)>>>,
    }

    impl MockCoreThreadDispatcher {
        async fn get_new_block_calls(&self) -> Vec<(Round, bool, Instant)> {
            let mut binding = self.new_block_calls.lock();
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

        async fn check_block_refs(
            &self,
            _block_refs: Vec<BlockRef>,
        ) -> Result<BTreeSet<BlockRef>, CoreError> {
            todo!()
        }

        async fn new_block(&self, round: Round, force: bool) -> Result<(), CoreError> {
            self.new_block_calls
                .lock()
                .push((round, force, Instant::now()));
            Ok(())
        }

        async fn get_missing_blocks(&self) -> Result<BTreeSet<BlockRef>, CoreError> {
            todo!()
        }

        fn set_subscriber_exists(&self, _exists: bool) -> Result<(), CoreError> {
            todo!()
        }

        fn set_propagation_delay_and_quorum_rounds(
            &self,
            _delay: Round,
            _received_quorum_rounds: Vec<QuorumRound>,
            _accepted_quorum_rounds: Vec<QuorumRound>,
        ) -> Result<(), CoreError> {
            todo!()
        }

        fn set_last_known_proposed_round(&self, _round: Round) -> Result<(), CoreError> {
            todo!()
        }

        fn highest_received_rounds(&self) -> Vec<Round> {
            todo!()
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn basic_leader_timeout() {
        let (context, _signers) = Context::new_for_test(4);
        let dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let leader_timeout = Duration::from_millis(500);
        let min_round_delay = Duration::from_millis(50);
        let parameters = Parameters {
            leader_timeout,
            min_round_delay,
            ..Default::default()
        };
        let context = Arc::new(context.with_parameters(parameters));
        let start = Instant::now();

        let (mut signals, signal_receivers) = CoreSignals::new(context.clone());

        // spawn the task
        let _handle = LeaderTimeoutTask::start(dispatcher.clone(), &signal_receivers, context);

        // send a signal that a new round has been produced.
        signals.new_round(10);

        // wait enough until the min round delay has passed and a new_block call is triggered
        sleep(2 * min_round_delay).await;
        let all_calls = dispatcher.get_new_block_calls().await;
        assert_eq!(all_calls.len(), 1);

        let (round, force, timestamp) = all_calls[0];
        assert_eq!(round, 10);
        assert!(!force);
        assert!(
            min_round_delay <= timestamp - start,
            "Leader timeout min setting {:?} should be less than actual time difference {:?}",
            min_round_delay,
            timestamp - start
        );

        // wait enough until a new_block has been received
        sleep(2 * leader_timeout).await;
        let all_calls = dispatcher.get_new_block_calls().await;
        assert_eq!(all_calls.len(), 1);

        let (round, force, timestamp) = all_calls[0];
        assert_eq!(round, 10);
        assert!(force);
        assert!(
            leader_timeout <= timestamp - start,
            "Leader timeout setting {:?} should be less than actual time difference {:?}",
            leader_timeout,
            timestamp - start
        );

        // now wait another 2 * leader_timeout, no other call should be received
        sleep(2 * leader_timeout).await;
        let all_calls = dispatcher.get_new_block_calls().await;

        assert_eq!(all_calls.len(), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn multiple_leader_timeouts() {
        let (context, _signers) = Context::new_for_test(4);
        let dispatcher = Arc::new(MockCoreThreadDispatcher::default());
        let leader_timeout = Duration::from_millis(500);
        let min_round_delay = Duration::from_millis(50);
        let parameters = Parameters {
            leader_timeout,
            min_round_delay,
            ..Default::default()
        };
        let context = Arc::new(context.with_parameters(parameters));
        let now = Instant::now();

        let (mut signals, signal_receivers) = CoreSignals::new(context.clone());

        // spawn the task
        let _handle = LeaderTimeoutTask::start(dispatcher.clone(), &signal_receivers, context);

        // now send some signals with some small delay between them, but not enough so every round
        // manages to timeout and call the force new block method.
        signals.new_round(13);
        sleep(min_round_delay / 2).await;
        signals.new_round(14);
        sleep(min_round_delay / 2).await;
        signals.new_round(15);
        sleep(2 * leader_timeout).await;

        // only the last one should be received
        let all_calls = dispatcher.get_new_block_calls().await;
        let (round, force, timestamp) = all_calls[0];
        assert_eq!(round, 15);
        assert!(!force);
        assert!(min_round_delay < timestamp - now);

        let (round, force, timestamp) = all_calls[1];
        assert_eq!(round, 15);
        assert!(force);
        assert!(leader_timeout < timestamp - now);
    }
}
