// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::peers::{SuiPeer, SuiPeers};

use sui_tls::Allower;

/// AllowWalrus will allow walrus nodes
#[derive(Debug, Clone, Default)]
pub struct WalrusProvider {
    peers: SuiPeers,
}

impl Allower for WalrusProvider {
    fn allowed(&self, key: &Ed25519PublicKey) -> bool {
        // Place whatever logic you need here, see the HashSetAllow in sui-tls::verifier
        // for a specific example.
        // eg you could do this:
        self.peers.read().unwrap().contains_key(key)
    }
}

impl WalrusProvider {
    pub fn new(peers: Vec<SuiPeer>) -> Self {
        // build our hashmap with the static pub keys. we only do this one time at binary startup.
        let statics: HashMap<Ed25519PublicKey, SuiPeer> = peers
            .into_iter()
            .map(|v| (v.public_key.clone(), v))
            .collect();
        Self { peers: statics }
    }
}
