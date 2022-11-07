// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
pub fn build_network(f: impl FnOnce(anemo::Router) -> anemo::Router) -> anemo::Network {
    let router = f(anemo::Router::new());
    let network = anemo::Network::bind("localhost:0")
        .private_key(random_key())
        .server_name("test")
        .start(router)
        .unwrap();

    println!(
        "starting network {} {}",
        network.local_addr(),
        network.peer_id(),
    );

    network
}

#[cfg(test)]
fn random_key() -> [u8; 32] {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rand::RngCore::fill_bytes(&mut rng, &mut bytes[..]);
    bytes
}
