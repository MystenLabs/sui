// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::node::NodeConfig;
use crate::{CommitConsumerMonitor, CommittedSubDag, ConsensusAuthority, TransactionClient};
use arc_swap::ArcSwapOption;
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use std::sync::Arc;
use tracing::info;

pub(crate) struct AuthorityNodeContainer {
    consensus_authority: ConsensusAuthority,
    commit_receiver: ArcSwapOption<UnboundedReceiver<CommittedSubDag>>,
    commit_consumer_monitor: ArcSwapOption<CommitConsumerMonitor>,
}

impl AuthorityNodeContainer {
    /// Spawn a new Node.
    pub async fn spawn(config: NodeConfig) -> Self {
        info!(index =% config.authority_index, "starting in-memory node non-sim");
        let (consensus_authority, commit_receiver, commit_consumer_monitor) =
            super::node::make_authority(config).await;

        Self {
            consensus_authority,
            commit_receiver: ArcSwapOption::new(Some(Arc::new(commit_receiver))),
            commit_consumer_monitor: ArcSwapOption::new(Some(commit_consumer_monitor)),
        }
    }

    pub fn take_commit_receiver(&self) -> UnboundedReceiver<CommittedSubDag> {
        let commit_receiver = self
            .commit_receiver
            .swap(None)
            .expect("Commit receiver already taken");
        let Ok(commit_receiver) = Arc::try_unwrap(commit_receiver) else {
            panic!("Commit receiver already consumed");
        };
        commit_receiver
    }

    pub fn commit_consumer_monitor(&self) -> Arc<CommitConsumerMonitor> {
        self.commit_consumer_monitor
            .load_full()
            .expect("Commit consumer monitor should be available")
            .clone()
    }

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        self.consensus_authority.transaction_client()
    }

    pub fn is_alive(&self) -> bool {
        true
    }
}
