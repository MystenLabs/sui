// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use crate::block_manager::BlockManager;
use consensus_config::{AuthorityIndex, Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
use prometheus::Registry;
use sui_protocol_config::ProtocolConfig;
use tracing::info;

use crate::block_verifier::BlockVerifier;
use crate::context::Context;
use crate::core::{Core, CoreSignals};
use crate::core_thread::CoreThreadDispatcher;
use crate::leader_timeout::{LeaderTimeoutTask, LeaderTimeoutTaskHandle};
use crate::metrics::initialise_metrics;
use crate::transactions_client::{TransactionsClient, TransactionsConsumer};

pub struct AuthorityNode {
    context: Arc<Context>,
    start_time: Instant,
    transactions_client: Arc<TransactionsClient>,
    leader_timeout_handle: LeaderTimeoutTaskHandle,
}

impl AuthorityNode {
    #[allow(unused)]
    async fn start(
        own_index: AuthorityIndex,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ProtocolConfig,
        // To avoid accidentally leaking the private key, the key pair should only be stored in core
        block_signer: NetworkKeyPair,
        _signer: ProtocolKeyPair,
        _block_verifier: impl BlockVerifier,
        registry: Registry,
    ) -> Self {
        info!("Starting authority with index {}", own_index);
        let context = Arc::new(Context::new(
            own_index,
            committee,
            parameters,
            protocol_config,
            initialise_metrics(registry),
        ));
        let start_time = Instant::now();

        // Create the transactions client and the transactions consumer
        let (client, tx_receiver) = TransactionsClient::new(context.clone());
        let tx_consumer = TransactionsConsumer::new(tx_receiver, context.clone(), None);

        // Construct Core
        let (core_signals, signals_receivers) = CoreSignals::new();
        let block_manager = BlockManager::new();
        let core = Core::new(
            context.clone(),
            tx_consumer,
            block_manager,
            core_signals,
            block_signer,
        );

        let (core_dispatcher, core_dispatcher_handle) =
            CoreThreadDispatcher::start(core, context.clone());
        let leader_timeout_handle =
            LeaderTimeoutTask::start(core_dispatcher, signals_receivers, context.clone());

        Self {
            context,
            start_time,
            leader_timeout_handle,
            transactions_client: Arc::new(client),
        }
    }

    #[allow(unused)]
    async fn stop(self) {
        info!(
            "Stopping authority. Total run time: {:?}",
            self.start_time.elapsed()
        );

        self.leader_timeout_handle.stop().await;

        self.context
            .metrics
            .node_metrics
            .uptime
            .observe(self.start_time.elapsed().as_secs_f64());
    }

    #[allow(unused)]
    pub fn transactions_client(&self) -> Arc<TransactionsClient> {
        self.transactions_client.clone()
    }
}

#[cfg(test)]
mod tests {
    use consensus_config::{Committee, NetworkKeyPair, Parameters, ProtocolKeyPair};
    use fastcrypto::traits::ToFromBytes;
    use prometheus::Registry;

    use crate::authority_node::AuthorityNode;
    use crate::block_verifier::TestBlockVerifier;
    use crate::context::Context;

    #[tokio::test]
    async fn start_and_stop() {
        let (committee, keypairs) = Committee::new_for_test(0, vec![1]);
        let registry = Registry::new();
        let parameters = Parameters::default();
        let block_verifier = TestBlockVerifier {};

        let (own_index, _) = committee.authorities().last().unwrap();
        let block_signer = NetworkKeyPair::from_bytes(keypairs[0].0.as_bytes()).unwrap();
        let signer = ProtocolKeyPair::from_bytes(keypairs[0].1.as_bytes()).unwrap();

        let authority = AuthorityNode::start(
            own_index,
            committee,
            parameters,
            Context::default_protocol_config_for_testing(),
            block_signer,
            signer,
            block_verifier,
            registry,
        )
        .await;

        assert_eq!(authority.context.own_index, own_index);
        assert_eq!(authority.context.committee.epoch(), 0);
        assert_eq!(authority.context.committee.size(), 1);

        authority.stop().await;
    }
}
