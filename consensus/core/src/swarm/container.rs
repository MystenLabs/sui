// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::node::NodeConfig;
use arc_swap::ArcSwapOption;
use consensus_config::Parameters;
use mysten_metrics::monitored_mpsc::UnboundedReceiver;
use prometheus::Registry;
use std::{sync::Arc, time::Duration};
use tracing::info;

use crate::transaction::NoopTransactionVerifier;
use crate::{
    CommitConsumer, CommitConsumerMonitor, CommittedSubDag, ConsensusAuthority, TransactionClient,
};

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
            Self::make_authority(config).await;

        Self {
            consensus_authority,
            commit_receiver: ArcSwapOption::new(Some(Arc::new(commit_receiver))),
            commit_consumer_monitor: ArcSwapOption::new(Some(commit_consumer_monitor)),
        }
    }

    pub fn take_commit_receiver(
        &self,
    ) -> (
        UnboundedReceiver<CommittedSubDag>,
        Arc<CommitConsumerMonitor>,
    ) {
        let commit_consumer_monitor = self
            .commit_consumer_monitor
            .swap(None)
            .expect("Commit consumer has been already consumed");
        let commit_receiver = self
            .commit_receiver
            .swap(None)
            .expect("Commit receiver already taken");
        let Ok(commit_receiver) = Arc::try_unwrap(commit_receiver) else {
            panic!("Commit receiver already consumed");
        };
        (commit_receiver, commit_consumer_monitor)
    }

    pub fn transaction_client(&self) -> Arc<TransactionClient> {
        self.consensus_authority.transaction_client()
    }

    pub fn is_alive(&self) -> bool {
        true
    }

    async fn make_authority(
        config: NodeConfig,
    ) -> (
        ConsensusAuthority,
        UnboundedReceiver<CommittedSubDag>,
        Arc<CommitConsumerMonitor>,
    ) {
        let NodeConfig {
            authority_index,
            db_dir,
            committee,
            keypairs,
            network_type,
            boot_counter,
            protocol_config,
        } = config;

        let registry = Registry::new();

        // Cache less blocks to exercise commit sync.
        let parameters = Parameters {
            db_path: db_dir.path().to_path_buf(),
            dag_state_cached_rounds: 5,
            commit_sync_parallel_fetches: 2,
            commit_sync_batch_size: 3,
            sync_last_known_own_block_timeout: Duration::from_millis(2_000),
            ..Default::default()
        };
        let txn_verifier = NoopTransactionVerifier {};

        let protocol_keypair = keypairs[authority_index].1.clone();
        let network_keypair = keypairs[authority_index].0.clone();

        let (commit_consumer, commit_receiver, _) = CommitConsumer::new(0);
        let commit_consumer_monitor = commit_consumer.monitor();

        let authority = ConsensusAuthority::start(
            network_type,
            authority_index,
            committee,
            parameters,
            protocol_config,
            protocol_keypair,
            network_keypair,
            Arc::new(txn_verifier),
            commit_consumer,
            registry,
            boot_counter,
        )
        .await;

        (authority, commit_receiver, commit_consumer_monitor)
    }
}
