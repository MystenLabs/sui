// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
pub fn build_network(f: impl FnOnce(anemo::Router) -> anemo::Router) -> anemo::Network {
    build_network_impl(f, None).0
}

#[cfg(test)]
pub fn build_network_and_key(
    f: impl FnOnce(anemo::Router) -> anemo::Router,
) -> (anemo::Network, sui_types::crypto::NetworkKeyPair) {
    build_network_impl(f, None)
}

#[cfg(test)]
pub fn build_network_with_anemo_config(
    f: impl FnOnce(anemo::Router) -> anemo::Router,
    anemo_config: anemo::Config,
) -> (anemo::Network, sui_types::crypto::NetworkKeyPair) {
    build_network_impl(f, Some(anemo_config))
}

#[cfg(test)]
fn build_network_impl(
    f: impl FnOnce(anemo::Router) -> anemo::Router,
    anemo_config: Option<anemo::Config>,
) -> (anemo::Network, sui_types::crypto::NetworkKeyPair) {
    use fastcrypto::traits::KeyPair;

    let keypair = sui_types::crypto::NetworkKeyPair::generate(&mut rand::thread_rng());
    let router = f(anemo::Router::new());
    let network = anemo::Network::bind("localhost:0")
        .private_key(keypair.copy().private().0.to_bytes())
        .config(anemo_config.unwrap_or_default())
        .server_name("test")
        .start(router)
        .unwrap();

    println!(
        "starting network {} {}",
        network.local_addr(),
        network.peer_id(),
    );
    (network, keypair)
}
