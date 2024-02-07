// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::codegen::BoxError;
use anemo::codegen::BoxFuture;
use anemo::codegen::Service;
use anemo::types::PeerEvent;
use anemo::Network;
use anemo::PeerId;
use anemo::Request;
use anemo::Response;
use bytes::Bytes;
use futures::future::OptionFuture;
use futures::FutureExt;
use std::time::Instant;

pub trait NetworkExt {
    fn waiting_peer(&self, peer_id: PeerId) -> WaitingPeer;
}

impl NetworkExt for Network {
    fn waiting_peer(&self, peer_id: PeerId) -> WaitingPeer {
        WaitingPeer::new(self.clone(), peer_id)
    }
}

#[derive(Clone)]
pub struct WaitingPeer {
    peer_id: PeerId,
    network: Network,
}

impl WaitingPeer {
    pub fn new(network: Network, peer_id: PeerId) -> Self {
        Self { peer_id, network }
    }

    async fn do_rpc(self, mut request: Request<Bytes>) -> Result<Response<Bytes>, BoxError> {
        use tokio::sync::broadcast::error::RecvError;

        let start = Instant::now();
        let (mut subscriber, _) = self.network.subscribe()?;

        // If we're connected with the peer immediately make the request
        if let Some(mut peer) = self.network.peer(self.peer_id) {
            return peer.rpc(request).await.map_err(Into::into);
        }

        // If we're not connected we'll need to check to see if the Peer is a KnownPeer
        let timeout = request.timeout();
        let sleep: OptionFuture<_> = timeout.map(tokio::time::sleep).into();
        tokio::pin!(sleep);
        loop {
            if self.network.known_peers().get(&self.peer_id).is_none() {
                return Err(format!("peer {} is not a known peer", self.peer_id).into());
            }

            tokio::select! {
                recv = subscriber.recv() => match recv {
                    Ok(PeerEvent::NewPeer(peer_id)) if peer_id == self.peer_id => {
                        // We're now connected with the peer, lets try to make a network request
                        if let Some(mut peer) = self.network.peer(self.peer_id) {
                            if let Some(duration) = timeout {
                                // Reduce timeout to account for time already spent waiting
                                // for the peer.
                                request.set_timeout(duration.saturating_sub(Instant::now().duration_since(start)));
                            }
                            return peer.rpc(request).await.map_err(Into::into);
                        }
                    }
                    Err(RecvError::Closed) => return Err("network is closed".into()),
                    Err(RecvError::Lagged(_)) => {
                        subscriber = subscriber.resubscribe();

                        // We lagged behind so we may have missed the connection event
                        if let Some(mut peer) = self.network.peer(self.peer_id) {
                            return peer.rpc(request).await.map_err(Into::into);
                        }
                    }
                    // Just do another iteration
                    _ => {}
                },
                Some(_) = &mut sleep => {
                    return Err(format!("timed out waiting for peer {}", self.peer_id).into());
                },
            }
        }
    }
}

impl Service<Request<Bytes>> for WaitingPeer {
    type Response = Response<Bytes>;
    type Error = BoxError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    #[inline]
    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    #[inline]
    fn call(&mut self, request: Request<Bytes>) -> Self::Future {
        let peer = self.clone();
        peer.do_rpc(request).boxed()
    }
}
