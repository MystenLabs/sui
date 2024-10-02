// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, RwLock};
use tokio::sync::watch;

use mysten_metrics::monitored_mpsc::UnboundedSender;

use crate::{CommitIndex, CommittedSubDag};

pub struct CommitConsumer {
    // A channel to send the committed sub dags through
    pub(crate) sender: UnboundedSender<CommittedSubDag>,
    // Index of the last commit that the consumer has processed. This is useful for
    // crash/recovery so mysticeti can replay the commits from the next index.
    // First commit in the replayed sequence will have index last_processed_commit_index + 1.
    // Set 0 to replay from the start (as generated commit sequence starts at index = 1).
    pub(crate) last_processed_commit_index: CommitIndex,
    // Allows the commit consumer to report its progress.
    monitor: Arc<CommitConsumerMonitor>,
}

impl CommitConsumer {
    pub fn new(
        sender: UnboundedSender<CommittedSubDag>,
        last_processed_commit_index: CommitIndex,
    ) -> Self {
        let monitor = Arc::new(CommitConsumerMonitor::new(last_processed_commit_index));
        Self {
            sender,
            last_processed_commit_index,
            monitor,
        }
    }

    pub fn monitor(&self) -> Arc<CommitConsumerMonitor> {
        self.monitor.clone()
    }
}

pub struct CommitConsumerMonitor {
    // highest commit that has been handled by Sui
    highest_handled_commit: watch::Sender<u32>,

    // the highest commit found in local storage at startup
    highest_observed_commit_at_startup: RwLock<u32>,
}

impl CommitConsumerMonitor {
    pub(crate) fn new(last_handled_commit: CommitIndex) -> Self {
        Self {
            highest_handled_commit: watch::Sender::new(last_handled_commit),
            highest_observed_commit_at_startup: RwLock::new(0),
        }
    }

    pub(crate) fn highest_handled_commit(&self) -> CommitIndex {
        *self.highest_handled_commit.borrow()
    }

    pub fn set_highest_handled_commit(&self, highest_handled_commit: CommitIndex) {
        self.highest_handled_commit
            .send_replace(highest_handled_commit);
    }

    pub fn highest_observed_commit_at_startup(&self) -> CommitIndex {
        *self.highest_observed_commit_at_startup.read().unwrap()
    }

    pub fn set_highest_observed_commit_at_startup(
        &self,
        highest_observed_commit_at_startup: CommitIndex,
    ) {
        let highest_handled_commit = self.highest_handled_commit();
        assert!(
            highest_observed_commit_at_startup >= highest_handled_commit,
            "we cannot have handled a commit that we do not know about: {} < {}",
            highest_observed_commit_at_startup,
            highest_handled_commit,
        );

        let mut commit = self.highest_observed_commit_at_startup.write().unwrap();

        assert!(
            *commit == 0,
            "highest_known_commit_at_startup can only be set once"
        );
        *commit = highest_observed_commit_at_startup;
    }

    pub(crate) async fn replay_complete(&self) {
        let highest_observed_commit_at_startup = self.highest_observed_commit_at_startup();
        let mut rx = self.highest_handled_commit.subscribe();
        loop {
            let highest_handled = *rx.borrow_and_update();
            if highest_handled >= highest_observed_commit_at_startup {
                return;
            }
            rx.changed().await.unwrap();
        }
    }
}

#[cfg(test)]
mod test {
    use crate::CommitConsumerMonitor;

    #[test]
    fn test_commit_consumer_monitor() {
        let monitor = CommitConsumerMonitor::new(10);
        assert_eq!(monitor.highest_handled_commit(), 10);

        monitor.set_highest_handled_commit(100);
        assert_eq!(monitor.highest_handled_commit(), 100);
    }
}
