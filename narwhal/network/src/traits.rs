// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use async_trait::async_trait;
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
    async fn unreliable_send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> JoinHandle<()>;

    async fn unreliable_send(
        &mut self,
        address: Multiaddr,
        message: &Self::Message,
    ) -> JoinHandle<()> {
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
    ) -> Vec<JoinHandle<()>> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = { self.unreliable_send_message(address, message.clone()).await };
            handlers.push(handle);
        }
        handlers
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
    ) -> Vec<JoinHandle<()>> {
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
