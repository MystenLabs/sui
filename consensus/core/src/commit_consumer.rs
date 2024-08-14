// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{atomic::AtomicU32, Arc};

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
    highest_handled_commit: AtomicU32,
}

impl CommitConsumerMonitor {
    pub(crate) fn new(last_handled_commit: CommitIndex) -> Self {
        Self {
            highest_handled_commit: AtomicU32::new(last_handled_commit),
        }
    }

    pub(crate) fn highest_handled_commit(&self) -> CommitIndex {
        self.highest_handled_commit
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn set_highest_handled_commit(&self, highest_handled_commit: CommitIndex) {
        self.highest_handled_commit
            .store(highest_handled_commit, std::sync::atomic::Ordering::Release);
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
