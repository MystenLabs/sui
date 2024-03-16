// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo_tower::auth::AuthorizeRequest;
use arc_swap::ArcSwap;
use bytes::Bytes;
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug)]
pub(crate) struct AllowedPeersUpdatable {
    allowed_peers: Arc<ArcSwap<HashSet<anemo::PeerId>>>,
}

impl AllowedPeersUpdatable {
    pub fn new(allowed_peers: Arc<HashSet<anemo::PeerId>>) -> Self {
        Self {
            allowed_peers: Arc::new(ArcSwap::new(allowed_peers)),
        }
    }

    pub fn update(&self, allowed_peers: Arc<HashSet<anemo::PeerId>>) {
        self.allowed_peers.store(allowed_peers);
    }
}

impl AuthorizeRequest for AllowedPeersUpdatable {
    fn authorize(&self, request: &mut anemo::Request<Bytes>) -> Result<(), anemo::Response<Bytes>> {
        use anemo::types::response::{IntoResponse, StatusCode};

        let peer_id = request
            .peer_id()
            .ok_or_else(|| StatusCode::InternalServerError.into_response())?;

        if self.allowed_peers.load().contains(peer_id) {
            Ok(())
        } else {
            Err(StatusCode::NotFound.into_response())
        }
    }
}
