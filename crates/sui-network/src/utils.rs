// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
pub fn build_network(f: impl FnOnce(anemo::Router) -> anemo::Router) -> anemo::Network {
    build_network_impl(f, None)
}

#[cfg(test)]
pub fn build_network_with_anemo_config(
    f: impl FnOnce(anemo::Router) -> anemo::Router,
    anemo_config: anemo::Config,
) -> anemo::Network {
    build_network_impl(f, Some(anemo_config))
}

#[cfg(test)]
fn build_network_impl(
    f: impl FnOnce(anemo::Router) -> anemo::Router,
    anemo_config: Option<anemo::Config>,
) -> anemo::Network {
    let router = f(anemo::Router::new());
    let network = anemo::Network::bind("localhost:0")
        .private_key(random_key())
        .config(anemo_config.unwrap_or_default())
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
