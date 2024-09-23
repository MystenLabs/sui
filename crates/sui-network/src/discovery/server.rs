// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Discovery, NodeInfo, State, MAX_PEERS_TO_SEND};
use anemo::{Request, Response};
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetKnownPeersResponse {
    pub own_info: NodeInfo,
    pub known_peers: Vec<NodeInfo>,
}

pub(super) struct Server {
    pub(super) state: Arc<RwLock<State>>,
}

#[anemo::async_trait]
impl Discovery for Server {
    async fn get_known_peers(
        &self,
        _request: Request<()>,
    ) -> Result<Response<GetKnownPeersResponse>, anemo::rpc::Status> {
        let state = self.state.read().unwrap();
        let own_info = state
            .our_info
            .clone()
            .ok_or_else(|| anemo::rpc::Status::internal("own_info has not been initialized yet"))?;

        let known_peers = if state.known_peers.len() < MAX_PEERS_TO_SEND {
            state.known_peers.values().cloned().collect()
        } else {
            let mut rng = rand::thread_rng();
            // prefer returning peers that we are connected to as they are known-good
            let mut known_peers = state
                .connected_peers
                .keys()
                .filter_map(|peer_id| state.known_peers.get(peer_id))
                .map(|info| (info.peer_id, info))
                .choose_multiple(&mut rng, MAX_PEERS_TO_SEND)
                .into_iter()
                .collect::<HashMap<_, _>>();

            if known_peers.len() <= MAX_PEERS_TO_SEND {
                // Fill the remaining space with other peers, randomly sampling at most MAX_PEERS_TO_SEND
                for info in state
                    .known_peers
                    .values()
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

            known_peers.into_values().cloned().collect()
        };

        Ok(Response::new(GetKnownPeersResponse {
            own_info,
            known_peers,
        }))
    }
}
