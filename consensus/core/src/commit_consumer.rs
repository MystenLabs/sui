// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use mysten_metrics::monitored_mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::watch;
use tracing::debug;

use crate::{CommitIndex, CommittedSubDag, block::CertifiedBlocksOutput};

/// Arguments from commit consumer to this consensus instance.
/// This includes both parameters and components for communications.
#[derive(Clone)]
pub struct CommitConsumerArgs {
    /// The consumer requests consensus to replay from commit replay_after_commit_index + 1.
    /// Set to 0 to replay from the start (as commit sequence starts at index = 1).
    pub(crate) replay_after_commit_index: CommitIndex,
    /// Index of the last commit that the consumer has processed.  This is useful during
    /// crash recovery when other components can wait for the consumer to finish processing
    /// up to this index.
    pub(crate) consumer_last_processed_commit_index: CommitIndex,

    /// A channel to output the committed sub dags.
    pub(crate) commit_sender: UnboundedSender<CommittedSubDag>,
    /// A channel to output blocks for processing, separated from consensus commits.
    /// In each block output, transactions that are not rejected are considered certified.
    pub(crate) block_sender: UnboundedSender<CertifiedBlocksOutput>,
    // Allows the commit consumer to report its progress.
    monitor: Arc<CommitConsumerMonitor>,
}

impl CommitConsumerArgs {
    pub fn new(
        replay_after_commit_index: CommitIndex,
        consumer_last_processed_commit_index: CommitIndex,
    ) -> (
        Self,
        UnboundedReceiver<CommittedSubDag>,
        UnboundedReceiver<CertifiedBlocksOutput>,
    ) {
        let (commit_sender, commit_receiver) = unbounded_channel("consensus_commit_output");
        let (block_sender, block_receiver) = unbounded_channel("consensus_block_output");

        let monitor = Arc::new(CommitConsumerMonitor::new(
            replay_after_commit_index,
            consumer_last_processed_commit_index,
        ));
        (
            Self {
                replay_after_commit_index,
                consumer_last_processed_commit_index,
                commit_sender,
                block_sender,
                monitor,
            },
            commit_receiver,
            block_receiver,
        )
    }

    pub fn monitor(&self) -> Arc<CommitConsumerMonitor> {
        self.monitor.clone()
    }
}

/// Helps monitor the progress of the consensus commit handler (consumer).
///
/// This component currently has two use usages:
/// 1. Checking the highest commit index processed by the consensus commit handler.
///    Consensus components can decide to wait for more commits to be processed before proceeding with
///    their work.
/// 2. Waiting for consensus commit handler to finish processing replayed commits.
///    Current usage is actually outside of consensus.
pub struct CommitConsumerMonitor {
    // highest commit that has been handled by the consumer.
    highest_handled_commit: watch::Sender<u32>,

    // At node startup, the last consensus commit processed by the commit consumer from the previous run.
    // This can be 0 if starting a new epoch.
    consumer_last_processed_commit_index: CommitIndex,
}

impl CommitConsumerMonitor {
    pub(crate) fn new(
        replay_after_commit_index: CommitIndex,
        consumer_last_processed_commit_index: CommitIndex,
    ) -> Self {
        Self {
            highest_handled_commit: watch::Sender::new(replay_after_commit_index),
            consumer_last_processed_commit_index,
        }
    }

    /// Gets the highest commit index processed by the consensus commit handler.
    pub fn highest_handled_commit(&self) -> CommitIndex {
        *self.highest_handled_commit.borrow()
    }

    /// Updates the highest commit index processed by the consensus commit handler.
    pub fn set_highest_handled_commit(&self, highest_handled_commit: CommitIndex) {
        debug!("Highest handled commit set to {}", highest_handled_commit);
        self.highest_handled_commit
            .send_replace(highest_handled_commit);
    }

    /// Waits for consensus to replay commits until the consumer last processed commit index.
    pub async fn replay_to_consumer_last_processed_commit_complete(&self) {
        let mut rx = self.highest_handled_commit.subscribe();
        loop {
            let highest_handled = *rx.borrow_and_update();
            if highest_handled >= self.consumer_last_processed_commit_index {
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
        let monitor = CommitConsumerMonitor::new(0, 10);
        assert_eq!(monitor.highest_handled_commit(), 0);

        monitor.set_highest_handled_commit(100);
        assert_eq!(monitor.highest_handled_commit(), 100);
    }
}
