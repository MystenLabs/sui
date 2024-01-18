// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use crate::block_manager::BlockManager;
use consensus_config::{AuthorityIndex, Committee, Parameters, ProtocolKeyPair};
use prometheus::Registry;
use sui_protocol_config::ProtocolConfig;
use tracing::info;

use crate::block_verifier::BlockVerifier;
use crate::context::Context;
use crate::core::{Core, CoreSignals};
use crate::metrics::initialise_metrics;
use crate::transactions_client::{TransactionsClient, TransactionsConsumer};

pub struct Validator {
    context: Arc<Context>,
    start_time: Instant,
    transactions_client: Arc<TransactionsClient>,
}

impl Validator {
    #[allow(unused)]
    async fn start(
        own_index: AuthorityIndex,
        committee: Committee,
        parameters: Parameters,
        protocol_config: ProtocolConfig,
        // To avoid accidentally leaking the private key, the key pair should only be
        // stored in the Block signer.
        _signer: ProtocolKeyPair,
        _block_verifier: impl BlockVerifier,
        registry: Registry,
    ) -> Self {
        info!("Boot validator with authority index {}", own_index);
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
        let tx_consumer = TransactionsConsumer::new(tx_receiver);

        // Construct Core
        let (core_signals, _signals_receivers) = CoreSignals::new();
        let block_manager = BlockManager::new();
        let _core = Core::new(context.clone(), tx_consumer, block_manager, core_signals);

        Self {
            context,
            start_time,
            transactions_client: Arc::new(client),
        }
    }

    #[allow(unused)]
    async fn stop(self) {
        info!(
            "Stopping validator. Total run time: {:?}",
            self.start_time.elapsed()
        );
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
    use crate::block_verifier::TestBlockVerifier;
    use crate::validator::Validator;
    use consensus_config::{Committee, Parameters, ProtocolKeyPair};
    use fastcrypto::traits::ToFromBytes;
    use prometheus::Registry;
    use sui_protocol_config::ProtocolConfig;

    #[tokio::test]
    async fn validator_start_and_stop() {
        let (committee, keypairs) = Committee::new_for_test(0, vec![1]);
        let registry = Registry::new();
        let parameters = Parameters::default();
        let block_verifier = TestBlockVerifier {};

        let (own_index, _) = committee.authorities().last().unwrap();
        let signer = ProtocolKeyPair::from_bytes(keypairs[0].1.as_bytes()).unwrap();

        let validator = Validator::start(
            own_index,
            committee,
            parameters,
            ProtocolConfig::get_for_min_version(),
            signer,
            block_verifier,
            registry,
        )
        .await;

        assert_eq!(validator.context.own_index, own_index);
        assert_eq!(validator.context.committee.epoch(), 0);
        assert_eq!(validator.context.committee.size(), 1);

        validator.stop().await;
    }
}
