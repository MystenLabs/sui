// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct P2pConfig {
    /// The address that the p2p network will bind on.
    #[serde(default = "default_listen_address")]
    pub listen_address: SocketAddr,
    /// The external address other nodes can use to reach this node.
    /// This will be shared with other peers through the discovery service
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_address: Option<Multiaddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anemo_config: Option<anemo::Config>,
}

fn default_listen_address() -> SocketAddr {
    "0.0.0.0:8080".parse().unwrap()
}

impl Default for P2pConfig {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            external_address: Default::default(),
            anemo_config: Default::default(),
        }
    }
}
