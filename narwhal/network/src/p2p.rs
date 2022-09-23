// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    traits::{Lucky, ReliableNetwork, UnreliableNetwork},
    BoundedExecutor, CancelOnDropHandler, RetryConfig, MAX_TASK_CONCURRENCY,
};
use anemo::PeerId;
use anyhow::Result;
use async_trait::async_trait;
use crypto::{traits::KeyPair, NetworkPublicKey};
use multiaddr::Multiaddr;
use rand::{rngs::SmallRng, SeedableRng as _};
use std::collections::HashMap;
use tokio::{runtime::Handle, task::JoinHandle};
use types::{
    PrimaryMessage, PrimaryToPrimaryClient, PrimaryToWorkerClient, PrimaryWorkerMessage,
    WorkerBatchRequest, WorkerBatchResponse, WorkerMessage, WorkerPrimaryMessage,
    WorkerToPrimaryClient, WorkerToWorkerClient,
};

fn default_executor() -> BoundedExecutor {
    BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current())
}

pub struct P2pNetwork {
    network: anemo::Network,
    retry_config: RetryConfig,
    /// Small RNG just used to shuffle nodes and randomize connections (not crypto related).
    rng: SmallRng,
    // One bounded executor per address
    executors: HashMap<PeerId, BoundedExecutor>,
}

impl P2pNetwork {
    pub fn new(network: anemo::Network) -> Self {
        let retry_config = RetryConfig {
            // Retry forever
            retrying_max_elapsed_time: None,
            ..Default::default()
        };

        Self {
            network,
            retry_config,
            rng: SmallRng::from_entropy(),
            executors: HashMap::new(),
        }
    }

    pub fn cleanup<'a, I>(&mut self, _to_remove: I)
    where
        I: IntoIterator<Item = &'a Multiaddr>,
    {
        // TODO This function was previously used to remove old clients on epoch changes. This may
        // not be necessary with the new networking stack so we'll need to revisit if this function
        // is even needed. For now do nothing.
    }

    // Creates a new single-use anemo::Network to connect outbound to a single
    // address. This is for tests and should not be used from worker code.
    pub async fn new_for_single_address(
        name: NetworkPublicKey,
        address: anemo::types::Address,
    ) -> Self {
        let routes = anemo::Router::new();
        let network = anemo::Network::bind("127.0.0.1:0")
            .server_name("narwhal")
            .private_key(
                crypto::NetworkKeyPair::generate(&mut rand::rngs::OsRng)
                    .private()
                    .0
                    .to_bytes(),
            )
            .start(routes)
            .unwrap();
        network
            .connect_with_peer_id(address, anemo::PeerId(name.0.to_bytes()))
            .await
            .unwrap();
        Self::new(network)
    }

    async fn unreliable_send<F, R, Fut>(
        &mut self,
        peer: NetworkPublicKey,
        f: F,
    ) -> Result<JoinHandle<Result<anemo::Response<R>>>>
    where
        F: FnOnce(anemo::Peer) -> Fut + Send + Sync + 'static,
        R: Send + Sync + 'static + Clone,
        Fut: std::future::Future<Output = Result<anemo::Response<R>, anemo::rpc::Status>> + Send,
    {
        let network = self.network.clone();
        let peer_id = PeerId(peer.0.to_bytes());
        Ok(self
            .executors
            .entry(peer_id)
            .or_insert_with(default_executor)
            .spawn(async move {
                if let Some(peer) = network.peer(peer_id) {
                    f(peer)
                        .await
                        .map_err(|e| anyhow::anyhow!("RPC error: {e:?}"))
                } else {
                    Err(anemo::Error::msg(format!(
                        "Network has no connection with peer {peer_id}"
                    )))
                }
            })
            .await)
    }

    async fn send<F, R, Fut>(
        &mut self,
        peer: NetworkPublicKey,
        f: F,
    ) -> CancelOnDropHandler<Result<anemo::Response<R>>>
    where
        F: Fn(anemo::Peer) -> Fut + Send + Sync + 'static + Clone,
        R: Send + Sync + 'static + Clone,
        Fut: std::future::Future<Output = Result<anemo::Response<R>, anemo::rpc::Status>> + Send,
    {
        // Safety
        // Since this spawns an unbounded task, this should be called in a time-restricted fashion.
        // Here the callers are [`PrimaryNetwork::broadcast`] and [`PrimaryNetwork::send`],
        // at respectively N and K calls per round.
        //  (where N is the number of primaries, K the number of workers for this primary)
        // See the TODO on spawn_with_retries for lifting this restriction.

        let network = self.network.clone();
        let peer_id = PeerId(peer.0.to_bytes());
        let message_send = move || {
            let network = network.clone();
            let f = f.clone();

            async move {
                if let Some(peer) = network.peer(peer_id) {
                    f(peer).await.map_err(|e| {
                        // this returns a backoff::Error::Transient
                        // so that if anemo::Status is returned, we retry
                        backoff::Error::transient(anyhow::anyhow!("RPC error: {e:?}"))
                    })
                } else {
                    Err(backoff::Error::transient(anyhow::anyhow!(
                        "not connected to peer {peer_id}"
                    )))
                }
            }
        };

        let handle = self
            .executors
            .entry(peer_id)
            .or_insert_with(default_executor)
            .spawn_with_retries(self.retry_config, message_send);

        CancelOnDropHandler(handle)
    }
}

impl Lucky for P2pNetwork {
    fn rng(&mut self) -> &mut SmallRng {
        &mut self.rng
    }
}

//
// Primary-to-Primary
//

#[async_trait]
impl UnreliableNetwork<PrimaryMessage> for P2pNetwork {
    type Response = ();
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &PrimaryMessage,
    ) -> Result<JoinHandle<Result<anemo::Response<()>>>> {
        let message = message.to_owned();
        let f = move |peer| async move {
            PrimaryToPrimaryClient::new(peer)
                .send_message(message)
                .await
        };
        self.unreliable_send(peer, f).await
    }
}

#[async_trait]
impl ReliableNetwork<PrimaryMessage> for P2pNetwork {
    type Response = ();
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &PrimaryMessage,
    ) -> CancelOnDropHandler<Result<anemo::Response<()>>> {
        let message = message.to_owned();
        let f = move |peer| {
            let message = message.clone();
            async move {
                PrimaryToPrimaryClient::new(peer)
                    .send_message(message)
                    .await
            }
        };

        self.send(peer, f).await
    }
}

//
// Primary-to-Worker
//

#[async_trait]
impl UnreliableNetwork<PrimaryWorkerMessage> for P2pNetwork {
    type Response = ();
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &PrimaryWorkerMessage,
    ) -> Result<JoinHandle<Result<anemo::Response<()>>>> {
        let message = message.to_owned();
        let f =
            move |peer| async move { PrimaryToWorkerClient::new(peer).send_message(message).await };
        self.unreliable_send(peer, f).await
    }
}

#[async_trait]
impl ReliableNetwork<PrimaryWorkerMessage> for P2pNetwork {
    type Response = ();
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &PrimaryWorkerMessage,
    ) -> CancelOnDropHandler<Result<anemo::Response<()>>> {
        let message = message.to_owned();
        let f = move |peer| {
            let message = message.clone();
            async move { PrimaryToWorkerClient::new(peer).send_message(message).await }
        };

        self.send(peer, f).await
    }
}

//
// Worker-to-Primary
//

#[async_trait]
impl UnreliableNetwork<WorkerPrimaryMessage> for P2pNetwork {
    type Response = ();
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerPrimaryMessage,
    ) -> Result<JoinHandle<Result<anemo::Response<()>>>> {
        let message = message.to_owned();
        let f =
            move |peer| async move { WorkerToPrimaryClient::new(peer).send_message(message).await };
        self.unreliable_send(peer, f).await
    }
}

#[async_trait]
impl ReliableNetwork<WorkerPrimaryMessage> for P2pNetwork {
    type Response = ();
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerPrimaryMessage,
    ) -> CancelOnDropHandler<Result<anemo::Response<()>>> {
        let message = message.to_owned();
        let f = move |peer| {
            let message = message.clone();
            async move { WorkerToPrimaryClient::new(peer).send_message(message).await }
        };

        self.send(peer, f).await
    }
}

//
// Worker-to-Worker
//

#[async_trait]
impl UnreliableNetwork<WorkerMessage> for P2pNetwork {
    type Response = ();
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerMessage,
    ) -> Result<JoinHandle<Result<anemo::Response<()>>>> {
        let message = message.to_owned();
        let f =
            move |peer| async move { WorkerToWorkerClient::new(peer).send_message(message).await };
        self.unreliable_send(peer, f).await
    }
}

#[async_trait]
impl ReliableNetwork<WorkerMessage> for P2pNetwork {
    type Response = ();
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerMessage,
    ) -> CancelOnDropHandler<Result<anemo::Response<()>>> {
        let message = message.to_owned();
        let f = move |peer| {
            let message = message.clone();
            async move { WorkerToWorkerClient::new(peer).send_message(message).await }
        };

        self.send(peer, f).await
    }
}

#[async_trait]
impl UnreliableNetwork<WorkerBatchRequest> for P2pNetwork {
    type Response = WorkerBatchResponse;
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerBatchRequest,
    ) -> Result<JoinHandle<Result<anemo::Response<WorkerBatchResponse>>>> {
        let message = message.to_owned();
        let f = move |peer| async move {
            WorkerToWorkerClient::new(peer)
                .request_batches(message)
                .await
        };
        self.unreliable_send(peer, f).await
    }
}

#[async_trait]
impl ReliableNetwork<WorkerBatchRequest> for P2pNetwork {
    type Response = WorkerBatchResponse;
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerBatchRequest,
    ) -> CancelOnDropHandler<Result<anemo::Response<WorkerBatchResponse>>> {
        let message = message.to_owned();
        let f = move |peer| {
            let message = message.clone();
            async move {
                WorkerToWorkerClient::new(peer)
                    .request_batches(message)
                    .await
            }
        };

        self.send(peer, f).await
    }
}
