// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod network;
pub mod sequencer;

use rand::rngs::StdRng;
use rand::SeedableRng;
use sui_types::base_types::SuiAddress;
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair_from_rng, KeyPair};

// Fixture: test key pairs.
pub fn test_keys() -> Vec<(SuiAddress, KeyPair)> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4).map(|_| get_key_pair_from_rng(&mut rng)).collect()
}

// Fixture: test committee.
pub fn test_committee() -> Committee {
    Committee::new(
        test_keys()
            .into_iter()
            .map(|(_, x)| (*x.public_key_bytes(), /* voting right */ 1))
            .collect(),
    )
}
