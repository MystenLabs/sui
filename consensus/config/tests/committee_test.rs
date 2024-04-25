// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use consensus_config::{
    Authority, AuthorityKeyPair, Committee, NetworkKeyPair, ProtocolKeyPair, Stake,
};
use insta::assert_yaml_snapshot;
use mysten_network::Multiaddr;
use rand::{rngs::StdRng, SeedableRng as _};

// Committee is not sent over network or stored on disk itself, but some of its fields are.
// So this test can still be useful to detect accidental format changes.
#[test]
fn committee_snapshot_matches() {
    let epoch = 100;

    let mut authorities: Vec<_> = vec![];
    let mut rng = StdRng::from_seed([9; 32]);
    let num_of_authorities = 10;
    for i in 1..=num_of_authorities {
        let authority_keypair = AuthorityKeyPair::generate(&mut rng);
        let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
        let network_keypair = NetworkKeyPair::generate(&mut rng);
        authorities.push(Authority {
            stake: i as Stake,
            address: Multiaddr::empty(),
            hostname: "test_host".to_string(),
            authority_key: authority_keypair.public(),
            protocol_key: protocol_keypair.public(),
            network_key: network_keypair.public(),
        });
    }

    let committee = Committee::new(epoch, authorities);

    assert_yaml_snapshot!("committee", committee)
}
