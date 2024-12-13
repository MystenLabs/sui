// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::node::NodeConfig;
use crate::{CommitConsumerMonitor, CommittedSubDag};
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use std::sync::Arc;
use tracing::info;

pub(crate) struct AuthorityNodeContainer {}

impl AuthorityNodeContainer {
    /// Spawn a new Node.
    pub async fn spawn(config: NodeConfig) -> Self {
        info!(index =% config.authority_index, "starting in-memory node non-sim");
        Self {}
    }

    pub fn take_commit_receiver(
        &self,
    ) -> (
        UnboundedReceiver<CommittedSubDag>,
        Arc<CommitConsumerMonitor>,
    ) {
        let (_tx, rx) = mysten_metrics::monitored_mpsc::unbounded_channel("commit_receiver_out");
        let commit_consumer_monitor = Arc::new(CommitConsumerMonitor::new(0));
        (rx, commit_consumer_monitor)
    }

    pub fn is_alive(&self) -> bool {
        true
    }
}
