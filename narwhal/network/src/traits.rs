// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
use crypto::NetworkPublicKey;
use multiaddr::Multiaddr;
use rand::prelude::{SliceRandom, SmallRng};
use serde::Serialize;
use tokio::task::JoinHandle;
use types::BincodeEncodedPayload;

use crate::{CancelOnDropHandler, MessageResult};

pub trait BaseNetwork {
    type Client;
    type Message: Serialize + Sync;

    fn client(&mut self, address: Multiaddr) -> Self::Client;
    fn create_client(config: &mysten_network::config::Config, address: Multiaddr) -> Self::Client;
}

#[async_trait]
pub trait UnreliableNetwork: BaseNetwork {
    /// Sends a serialized message to a network destination
    /// Implementations of this method must not block on I/O.
    async fn unreliable_send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> ();

    async fn unreliable_send(&mut self, address: Multiaddr, message: &Self::Message) -> () {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        self.unreliable_send_message(address, message).await
    }

    /// Broadcasts a message to all `addresses` passed as an argument.
    /// The attempts to send individual messages are best effort and will not be retried.
    async fn unreliable_broadcast(
        &mut self,
        addresses: Vec<Multiaddr>,
        message: &Self::Message,
    ) -> () {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        for address in addresses {
            // this is ok assuming implementations make unreliable_send_message non-blocking
            self.unreliable_send_message(address, message.clone()).await
        }
    }
}

#[async_trait]
pub trait LuckyNetwork: UnreliableNetwork {
    fn rng(&mut self) -> &mut SmallRng;

    /// Pick a few addresses at random (specified by `nodes`) and try (best-effort) to send the
    /// message only to them. This is useful to pick nodes with whom to sync.
    async fn lucky_broadcast(
        &mut self,
        mut addresses: Vec<Multiaddr>,
        message: &Self::Message,
        nodes: usize,
    ) -> () {
        addresses.shuffle(self.rng());
        addresses.truncate(nodes);
        self.unreliable_broadcast(addresses, message).await
    }
}

#[async_trait]
pub trait ReliableNetwork: BaseNetwork {
    async fn send(
        &mut self,
        address: Multiaddr,
        message: &Self::Message,
    ) -> CancelOnDropHandler<MessageResult> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        self.send_message(address, message).await
    }

    async fn send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> CancelOnDropHandler<MessageResult>;

    async fn broadcast(
        &mut self,
        addresses: Vec<Multiaddr>,
        message: &Self::Message,
    ) -> Vec<CancelOnDropHandler<MessageResult>> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = self.send_message(address, message.clone()).await;
            handlers.push(handle);
        }
        handlers
    }
}

#[async_trait]
pub trait UnreliableNetwork2<Message: Clone + Send + Sync> {
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &Message,
    ) -> JoinHandle<()>;

    /// Broadcasts a message to all `peers` passed as an argument.
    /// The attempts to send individual messages are best effort and will not be retried.
    async fn unreliable_broadcast(
        &mut self,
        peers: Vec<NetworkPublicKey>,
        message: &Message,
    ) -> Vec<JoinHandle<()>> {
        let mut handlers = Vec::new();
        for peer in peers {
            let handle = { self.unreliable_send(peer, message).await };
            handlers.push(handle);
        }
        handlers
    }
}

pub trait Lucky {
    fn rng(&mut self) -> &mut SmallRng;
}

#[async_trait]
pub trait LuckyNetwork2<Message> {
    /// Pick a few addresses at random (specified by `nodes`) and try (best-effort) to send the
    /// message only to them. This is useful to pick nodes with whom to sync.
    async fn lucky_broadcast(
        &mut self,
        mut peers: Vec<NetworkPublicKey>,
        message: &Message,
        num_nodes: usize,
    ) -> Vec<JoinHandle<()>>;
}

#[async_trait]
impl<T, M> LuckyNetwork2<M> for T
where
    M: Clone + Send + Sync,
    T: UnreliableNetwork2<M> + Send,
    T: Lucky,
{
    async fn lucky_broadcast(
        &mut self,
        mut peers: Vec<NetworkPublicKey>,
        message: &M,
        nodes: usize,
    ) -> Vec<JoinHandle<()>> {
        peers.shuffle(self.rng());
        peers.truncate(nodes);
        self.unreliable_broadcast(peers, message).await
    }
}

#[async_trait]
pub trait ReliableNetwork2<Message: Clone + Send + Sync> {
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &Message,
    ) -> CancelOnDropHandler<anyhow::Result<anemo::Response<()>>>;

    async fn broadcast(
        &mut self,
        peers: Vec<NetworkPublicKey>,
        message: &Message,
    ) -> Vec<CancelOnDropHandler<anyhow::Result<anemo::Response<()>>>> {
        let mut handlers = Vec::new();
        for peer in peers {
            let handle = self.send(peer, message).await;
            handlers.push(handle);
        }
        handlers
    }
}
