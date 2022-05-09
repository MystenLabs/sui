// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{CancelHandler, RetryConfig};
use crypto::traits::VerifyingKey;
use futures::FutureExt;
use multiaddr::Multiaddr;
use rand::{prelude::SliceRandom as _, rngs::SmallRng, SeedableRng as _};
use std::collections::HashMap;
use tokio::task::JoinHandle;
use tonic::transport::Channel;
use types::{
    BincodeEncodedPayload, PrimaryMessage, PrimaryToPrimaryClient, PrimaryToWorkerClient,
    PrimaryWorkerMessage,
};

pub struct PrimaryNetwork {
    clients: HashMap<Multiaddr, PrimaryToPrimaryClient<Channel>>,
    config: mysten_network::config::Config,
    retry_config: RetryConfig,
    /// Small RNG just used to shuffle nodes and randomize connections (not crypto related).
    rng: SmallRng,
}

impl Default for PrimaryNetwork {
    fn default() -> Self {
        let retry_config = RetryConfig {
            // Retry for forever
            retrying_max_elapsed_time: None,
            ..Default::default()
        };

        Self {
            clients: Default::default(),
            config: Default::default(),
            retry_config,
            rng: SmallRng::from_entropy(),
        }
    }
}

impl PrimaryNetwork {
    fn client(&mut self, address: Multiaddr) -> PrimaryToPrimaryClient<Channel> {
        self.clients
            .entry(address.clone())
            .or_insert_with(|| Self::create_client(&self.config, address))
            .clone()
    }

    fn create_client(
        config: &mysten_network::config::Config,
        address: Multiaddr,
    ) -> PrimaryToPrimaryClient<Channel> {
        //TODO don't panic here if address isn't supported
        let channel = config.connect_lazy(&address).unwrap();
        PrimaryToPrimaryClient::new(channel)
    }

    pub async fn send<T: VerifyingKey>(
        &mut self,
        address: Multiaddr,
        message: &PrimaryMessage<T>,
    ) -> CancelHandler<()> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        self.send_message(address, message).await
    }

    async fn send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> CancelHandler<()> {
        let client = self.client(address);
        let handle = tokio::spawn(
            self.retry_config
                .retry(move || {
                    let mut client = client.clone();
                    let message = message.clone();
                    async move { client.send_message(message).await.map_err(Into::into) }
                })
                .map(|response| {
                    response.expect("we retry forever so this shouldn't fail");
                }),
        );

        CancelHandler(handle)
    }

    pub async fn broadcast<T: VerifyingKey>(
        &mut self,
        addresses: Vec<Multiaddr>,
        message: &PrimaryMessage<T>,
    ) -> Vec<CancelHandler<()>> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = self.send_message(address, message.clone()).await;
            handlers.push(handle);
        }
        handlers
    }

    pub async fn unreliable_send<T: VerifyingKey>(
        &mut self,
        address: Multiaddr,
        message: &PrimaryMessage<T>,
    ) -> JoinHandle<()> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut client = self.client(address);
        tokio::spawn(async move {
            let _ = client.send_message(message).await;
        })
    }

    /// Pick a few addresses at random (specified by `nodes`) and try (best-effort) to send the
    /// message only to them. This is useful to pick nodes with whom to sync.
    pub async fn unreliable_broadcast<T: VerifyingKey>(
        &mut self,
        addresses: Vec<Multiaddr>,
        message: &PrimaryMessage<T>,
    ) -> Vec<JoinHandle<()>> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = {
                let mut client = self.client(address);
                let message = message.clone();
                tokio::spawn(async move {
                    let _ = client.send_message(message).await;
                })
            };
            handlers.push(handle);
        }
        handlers
    }

    /// Pick a few addresses at random (specified by `nodes`) and try (best-effort) to send the
    /// message only to them. This is useful to pick nodes with whom to sync.
    pub async fn lucky_broadcast<T: VerifyingKey>(
        &mut self,
        mut addresses: Vec<Multiaddr>,
        message: &PrimaryMessage<T>,
        nodes: usize,
    ) -> Vec<JoinHandle<()>> {
        addresses.shuffle(&mut self.rng);
        addresses.truncate(nodes);
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = {
                let mut client = self.client(address);
                let message = message.clone();
                tokio::spawn(async move {
                    let _ = client.send_message(message).await;
                })
            };
            handlers.push(handle);
        }
        handlers
    }
}

#[derive(Default)]
pub struct PrimaryToWorkerNetwork {
    clients: HashMap<Multiaddr, PrimaryToWorkerClient<Channel>>,
    config: mysten_network::config::Config,
}

impl PrimaryToWorkerNetwork {
    fn client(&mut self, address: Multiaddr) -> PrimaryToWorkerClient<Channel> {
        self.clients
            .entry(address.clone())
            .or_insert_with(|| Self::create_client(&self.config, address))
            .clone()
    }

    fn create_client(
        config: &mysten_network::config::Config,
        address: Multiaddr,
    ) -> PrimaryToWorkerClient<Channel> {
        //TODO don't panic here if address isn't supported
        let channel = config.connect_lazy(&address).unwrap();
        PrimaryToWorkerClient::new(channel)
    }

    pub async fn send<T: VerifyingKey>(
        &mut self,
        address: Multiaddr,
        message: &PrimaryWorkerMessage<T>,
    ) -> JoinHandle<()> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut client = self.client(address);
        tokio::spawn(async move {
            let _ = client.send_message(message).await;
        })
    }

    pub async fn broadcast<T: VerifyingKey>(
        &mut self,
        addresses: Vec<Multiaddr>,
        message: &PrimaryWorkerMessage<T>,
    ) -> Vec<JoinHandle<()>> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = {
                let mut client = self.client(address);
                let message = message.clone();
                tokio::spawn(async move {
                    let _ = client.send_message(message).await;
                })
            };
            handlers.push(handle);
        }
        handlers
    }
}
