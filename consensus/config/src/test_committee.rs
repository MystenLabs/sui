// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{TcpListener, TcpStream};

use mysten_network::Multiaddr;
use rand::{rngs::StdRng, SeedableRng as _};

use crate::{
    Authority, AuthorityKeyPair, Committee, Epoch, NetworkKeyPair, ProtocolKeyPair, Stake,
};

/// Creates a committee for local testing, and the corresponding key pairs for the authorities.
pub fn local_committee_and_keys(
    epoch: Epoch,
    authorities_stake: Vec<Stake>,
) -> (Committee, Vec<(NetworkKeyPair, ProtocolKeyPair)>) {
    let mut authorities = vec![];
    let mut key_pairs = vec![];
    let mut rng = StdRng::from_seed([0; 32]);
    for (i, stake) in authorities_stake.into_iter().enumerate() {
        let authority_keypair = AuthorityKeyPair::generate(&mut rng);
        let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
        let network_keypair = NetworkKeyPair::generate(&mut rng);
        authorities.push(Authority {
            stake,
            address: get_available_local_address(),
            hostname: format!("test_host_{i}").to_string(),
            authority_key: authority_keypair.public(),
            protocol_key: protocol_keypair.public(),
            network_key: network_keypair.public(),
        });
        key_pairs.push((network_keypair, protocol_keypair));
    }

    let committee = Committee::new(epoch, authorities);
    (committee, key_pairs)
}

/// Returns a local address with an ephemeral port.
fn get_available_local_address() -> Multiaddr {
    let host = "127.0.0.1";
    let port = get_available_port(host);
    format!("/ip4/{}/udp/{}", host, port).parse().unwrap()
}

/// Returns an ephemeral, available port. On unix systems, the port returned will be in the
/// TIME_WAIT state ensuring that the OS won't hand out this port for some grace period.
/// Callers should be able to bind to this port given they use SO_REUSEADDR.
fn get_available_port(host: &str) -> u16 {
    const MAX_PORT_RETRIES: u32 = 1000;

    for _ in 0..MAX_PORT_RETRIES {
        if let Ok(port) = get_ephemeral_port(host) {
            return port;
        }
    }

    panic!("Error: could not find an available port");
}

fn get_ephemeral_port(host: &str) -> std::io::Result<u16> {
    // Request a random available port from the OS
    let listener = TcpListener::bind((host, 0))?;
    let addr = listener.local_addr()?;

    // Create and accept a connection (which we'll promptly drop) in order to force the port
    // into the TIME_WAIT state, ensuring that the port will be reserved from some limited
    // amount of time (roughly 60s on some Linux systems)
    let _sender = TcpStream::connect(addr)?;
    let _incoming = listener.accept()?;

    Ok(addr.port())
}
