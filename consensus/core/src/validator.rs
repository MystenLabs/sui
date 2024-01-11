// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Instant;

use consensus_config::{AuthorityIndex, Committee, Parameters, ProtocolKeyPair};
use prometheus::Registry;
use sui_protocol_config::ProtocolConfig;
use tracing::info;

use crate::block_verifier::BlockVerifier;
use crate::context::Context;
use crate::metrics::initialise_metrics;

pub struct Validator {
    context: Arc<Context>,
    start_time: Instant,
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

        Self {
            context,
            start_time,
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
