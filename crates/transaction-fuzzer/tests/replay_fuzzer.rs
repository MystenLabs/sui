// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::thread_rng;
use sui_replay::fuzz::{ReplayFuzzer, ReplayFuzzerConfig, ShuffleMutator};
const TESTNET_FULLNODE_URL: &str = "https://fullnode.testnet.sui.io:443";
#[tokio::test]
async fn test_replay_fuzzer() {
    let config = ReplayFuzzerConfig {
        // TODO: auto pick a recent range in testnet due to pruning
        checkpoint_id_start: Some(2_000_000),
        checkpoint_id_end: Some(2_000_100),
        num_mutations_per_base: 4,
        mutator: Box::new(ShuffleMutator {
            rng: thread_rng(),
            num_mutations_per_base_left: 3,
        }),
    };
    let fuzzer = ReplayFuzzer::new(TESTNET_FULLNODE_URL.to_string(), None, config)
        .await
        .unwrap();
    fuzzer.run(4).await.unwrap();
}
