// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
pub mod authority;
pub mod messages;
pub mod network;
pub mod objects;
pub mod transaction;
use rand::{rngs::StdRng, SeedableRng};
use sui_types::{
    base_types::SuiAddress,
    committee::Committee,
    crypto::{
        get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, AuthorityPublicKeyBytes,
        KeypairTraits,
    },
    signature_seed::SignatureSeed,
};

/// The size of the committee used for tests.
pub const TEST_COMMITTEE_SIZE: usize = 4;

/// Generate `COMMITTEE_SIZE` test cryptographic key pairs.
pub fn test_keys() -> Vec<(SuiAddress, AuthorityKeyPair)> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..TEST_COMMITTEE_SIZE)
        .map(|_| get_key_pair_from_rng(&mut rng))
        .collect()
}

/// Generate `COMMITTEE_SIZE` test cryptographic key pairs.
pub fn test_account_keys() -> Vec<(SuiAddress, AccountKeyPair)> {
    let mut vec = Vec::new();
    let ss = SignatureSeed::from_bytes(&[0; 32]).unwrap();
    for i in 0..TEST_COMMITTEE_SIZE {
        let kp: AccountKeyPair = ss.new_deterministic_keypair(&[i as u8], Some(&[])).unwrap();
        vec.push((kp.public().into(), kp));
    }
    vec
}

/// Generate a test Sui committee with `TEST_COMMITTEE_SIZE` members.
pub fn test_committee() -> Committee {
    Committee::new(
        0,
        test_keys()
            .into_iter()
            .map(|(_, x)| {
                (
                    AuthorityPublicKeyBytes::from(x.public()),
                    /* voting right */ 1,
                )
            })
            .collect(),
    )
    .unwrap()
}
