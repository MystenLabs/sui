// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::prelude::*;
use proptest::strategy::Strategy;

pub fn dalek_keypair_strategy() -> impl Strategy<Value = ed25519_dalek::Keypair> {
    any::<[u8; 32]>()
        .prop_map(|seed| {
            let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_seed(seed);
            ed25519_dalek::Keypair::generate(&mut rng)
        })
        .no_shrink()
}

pub fn dalek_pubkey_strategy() -> impl Strategy<Value = ed25519_dalek::PublicKey> {
    dalek_keypair_strategy().prop_map(|v| v.public)
}
