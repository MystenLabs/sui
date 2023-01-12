// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::Network;
use prometheus::Registry;
use std::net::SocketAddr;
use sui_config::p2p::{P2pConfig, SeedPeer, StateSyncConfig};
use sui_config::utils;
use sui_network::create_p2p_network;
use sui_network::state_sync::Handle;
use sui_types::committee::{Committee, EpochId};
use sui_types::crypto::NetworkKeyPair;
use sui_types::error::SuiResult;
use sui_types::storage::{ReadStore, WriteStore};

pub struct LightClient<S> {
    network: Network,
    state_sync_store: S,
    state_sync_handle: Handle,
}

impl<S> LightClient<S>
where
    S: WriteStore + Clone + Send + Sync + 'static,
    <S as ReadStore>::Error: std::error::Error,
{
    pub fn new(
        state_sync_store: S,
        p2p_address: SocketAddr,
        seed_peers: Vec<SeedPeer>,
        network_key_pair: NetworkKeyPair,
        prometheus_registry: &Registry,
    ) -> anyhow::Result<Self> {
        let p2p_config = P2pConfig {
            listen_address: p2p_address,
            external_address: Some(utils::socket_address_to_udp_multiaddr(p2p_address)),
            seed_peers,
            state_sync: Some(StateSyncConfig {
                disable_checkpoint_sync: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let (network, _, state_sync_handle) = create_p2p_network(
            p2p_config,
            state_sync_store.clone(),
            network_key_pair,
            prometheus_registry,
        )?;
        Ok(Self {
            network,
            state_sync_store,
            state_sync_handle,
        })
    }

    pub fn get_committee(&self, epoch: EpochId) -> Result<Option<Committee>, S::Error> {
        self.state_sync_store.get_committee(epoch)
    }
}
