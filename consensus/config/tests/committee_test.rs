// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_config::{CommitteeBuilder, NetworkKeyPair, ProtocolKeyPair, Stake};
use fastcrypto::traits::KeyPair as _;
use insta::assert_yaml_snapshot;
use multiaddr::Multiaddr;
use rand::{rngs::StdRng, SeedableRng as _};

#[test]
fn committee_snapshot_matches() {
    let mut rng = StdRng::from_seed([9; 32]);
    let num_of_authorities = 10;

    let mut committee_builder = CommitteeBuilder::new(10);

    for i in 1..=num_of_authorities {
        let network_keypair = NetworkKeyPair::generate(&mut rng);
        let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
        committee_builder.add_authority(
            i as Stake,
            Multiaddr::empty(),
            "test_host".to_string(),
            network_keypair.public().clone(),
            protocol_keypair.public().clone(),
        );
    }

    let committee = committee_builder.build();
    assert_yaml_snapshot!("committee", committee)
}
