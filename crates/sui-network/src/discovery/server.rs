// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{Discovery, NodeInfo, State};
use anemo::{Request, Response};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

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
        let known_peers = state.known_peers.values().cloned().collect();

        Ok(Response::new(GetKnownPeersResponse {
            own_info,
            known_peers,
        }))
    }
}
