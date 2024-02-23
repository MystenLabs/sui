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
        let mut leader_round_timed_out = false;
        let timer_start = Instant::now();
        let leader_timeout = sleep_until(timer_start + self.leader_timeout);

        tokio::pin!(leader_timeout);

        loop {
            tokio::select! {
                // when leader timer expires then we attempt to trigger the creation of a new block.
                // If we already timed out before then the branch gets disabled so we don't attempt
                // all the time to produce already produced blocks for that round.
                () = &mut leader_timeout, if !leader_round_timed_out => {
                    if let Err(err) = self.dispatcher.force_new_block(leader_round).await {
                        warn!("Error received while calling dispatcher, probably dispatcher is shutting down, will now exit: {err:?}");
                        return;
                    }
                    leader_round_timed_out = true;
                }

                // a new round has been produced. Reset the leader timeout.
                Ok(_) = new_round.changed() => {
                    leader_round = *new_round.borrow_and_update();
                    debug!("New round has been received {leader_round}, resetting timer");

                    leader_round_timed_out = false;

                    leader_timeout
                    .as_mut()
                    .reset(Instant::now() + self.leader_timeout);
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

        let (mut signals, signal_receivers) = CoreSignals::new();

        // spawn the task
        let _handle = LeaderTimeoutTask::start(dispatcher.clone(), &signal_receivers, context);

        // send a signal that a new round has been produced.
        signals.new_round(10).unwrap();

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

        let (mut signals, signal_receivers) = CoreSignals::new();

        // spawn the task
        let _handle = LeaderTimeoutTask::start(dispatcher.clone(), &signal_receivers, context);

        // now send some signals with some small delay between them, but not enough so every round
        // manages to timeout and call the force new block method.
        signals.new_round(13).unwrap();
        sleep(leader_timeout / 2).await;
        signals.new_round(14).unwrap();
        sleep(leader_timeout / 2).await;
        signals.new_round(15).unwrap();
        sleep(2 * leader_timeout).await;

        // only the last one should be received
        let all_calls = dispatcher.get_force_new_block_calls().await;
        let (round, timestamp) = all_calls[0];
        assert_eq!(round, 15);
        assert!(leader_timeout < timestamp - now);
    }
}
