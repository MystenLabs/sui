// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::block_validator::BlockValidator;
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
    fn start(
        authority: AuthorityIndex,
        _committee: Committee,
        _parameters: Parameters,
        _signer: ProtocolKeyPair,
        _block_validator: impl BlockValidator,
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
    fn stop(self) {
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
    use crate::block_validator::AcceptAllBlockValidator;
    use crate::validator::Validator;
    use consensus_config::{AuthorityIndex, Committee, Parameters, ProtocolKeyPair};
    use fastcrypto::traits::ToFromBytes;
    use prometheus::Registry;

    #[tokio::test]
    async fn validator_start_and_stop() {
        let committee = Committee::new(0, vec![]);
        let registry = Registry::new();
        let parameters = Parameters::default();
        let signer = ProtocolKeyPair::from_bytes(&[0u8; 32]).unwrap();
        let block_validator = AcceptAllBlockValidator {};

        let validator = Validator::start(
            AuthorityIndex(0),
            committee,
            parameters,
            signer,
            block_validator,
            registry,
        );

        validator.stop();
    }
}
