// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block_verifier::BlockVerifier;
use crate::metrics::{initialise_metrics, Metrics};
use consensus_config::{AuthorityIndex, Committee, Parameters, ProtocolKeyPair};
use prometheus::Registry;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

pub struct Validator {
    start_time: Instant,
    metrics: Arc<Metrics>,
}

impl Validator {
    #[allow(unused)]
    async fn start(
        authority: AuthorityIndex,
        _committee: Committee,
        _parameters: Parameters,
        _signer: ProtocolKeyPair,
        _block_verifier: impl BlockVerifier,
        registry: Registry,
    ) -> Self {
        info!("Boot validator with index {}", authority);
        let metrics = initialise_metrics(registry);
        let start_time = Instant::now();

        Self {
            start_time,
            metrics,
        }
    }

    #[allow(unused)]
    async fn stop(self) {
        info!(
            "Stopping validator. Total run time: {:?}",
            self.start_time.elapsed()
        );
        self.metrics
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

    #[tokio::test]
    async fn validator_start_and_stop() {
        let (committee, keypairs) = Committee::new_for_test(0, 1);
        let registry = Registry::new();
        let parameters = Parameters::default();
        let block_verifier = TestBlockVerifier {};

        let (authority_index, _) = committee.authorities().last().unwrap();
        let singer = ProtocolKeyPair::from_bytes(keypairs[0].1.as_bytes()).unwrap();

        let validator = Validator::start(
            authority_index,
            committee,
            parameters,
            singer,
            block_verifier,
            registry,
        )
        .await;

        validator.stop().await;
    }
}
