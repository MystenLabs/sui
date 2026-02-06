// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Discovery, MAX_PEERS_TO_SEND, SignedNodeInfo, State};
use anemo::{PeerId, Request, Response, types::PeerInfo};
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};
use sui_config::p2p::AccessType;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetKnownPeersResponseV2 {
    pub own_info: SignedNodeInfo,
    pub known_peers: Vec<SignedNodeInfo>,
}

pub(super) struct Server {
    pub(super) state: Arc<RwLock<State>>,
    pub(super) configured_peers: Arc<OnceLock<HashMap<PeerId, PeerInfo>>>,
}

#[anemo::async_trait]
impl Discovery for Server {
    async fn get_known_peers_v2(
        &self,
        request: Request<()>,
    ) -> Result<Response<GetKnownPeersResponseV2>, anemo::rpc::Status> {
        let state = self.state.read().unwrap();
        let own_info = state
            .our_info
            .clone()
            .ok_or_else(|| anemo::rpc::Status::internal("own_info has not been initialized yet"))?;

        let should_share = |info: &super::VerifiedSignedNodeInfo| match info.access_type {
            AccessType::Public => true,
            AccessType::Private => false,
            AccessType::Trusted => {
                // Share Trusted peers only with other preconfigured peers.
                self.configured_peers
                    .get()
                    .and_then(|configured_peers| {
                        request
                            .peer_id()
                            .map(|id| configured_peers.contains_key(id))
                    })
                    .unwrap_or(false)
            }
        };

        let known_peers = if state.known_peers.len() < MAX_PEERS_TO_SEND {
            state
                .known_peers
                .values()
                .filter(|e| should_share(e))
                .map(|e| e.inner())
                .cloned()
                .collect()
        } else {
            let mut rng = rand::thread_rng();
            // prefer returning peers that we are connected to as they are known-good
            let mut known_peers = state
                .connected_peers
                .keys()
                .filter_map(|peer_id| state.known_peers.get(peer_id))
                .filter(|info| should_share(info))
                .map(|info| (info.peer_id, info))
                .choose_multiple(&mut rng, MAX_PEERS_TO_SEND)
                .into_iter()
                .collect::<HashMap<_, _>>();

            if known_peers.len() <= MAX_PEERS_TO_SEND {
                // Fill the remaining space with other peers, randomly sampling at most MAX_PEERS_TO_SEND
                for info in state
                    .known_peers
                    .values()
                    .filter(|info| should_share(info))
                    // This randomly samples the iterator stream but the order of elements after
                    // sampling may not be random, this is ok though since we're just trying to do
                    // best-effort on sharing info of peers we haven't connected with ourselves.
                    .choose_multiple(&mut rng, MAX_PEERS_TO_SEND)
                {
                    if known_peers.len() >= MAX_PEERS_TO_SEND {
                        break;
                    }

                    known_peers.insert(info.peer_id, info);
                }
            }

            known_peers
                .into_values()
                .map(|e| e.inner())
                .cloned()
                .collect()
        };

        Ok(Response::new(GetKnownPeersResponseV2 {
            own_info,
            known_peers,
        }))
    }
}
