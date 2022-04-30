// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{CancelHandler, RetryConfig};
use crypto::traits::VerifyingKey;
use futures::FutureExt;
use rand::{prelude::SliceRandom as _, rngs::SmallRng, SeedableRng as _};
use std::{collections::HashMap, net::SocketAddr};
use tokio::task::JoinHandle;
use tonic::transport::Channel;
use types::{
    BincodeEncodedPayload, PrimaryMessage, PrimaryToPrimaryClient, PrimaryToWorkerClient,
    PrimaryWorkerMessage,
};

pub struct PrimaryNetwork {
    clients: HashMap<SocketAddr, PrimaryToPrimaryClient<Channel>>,
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
            retry_config,
            rng: SmallRng::from_entropy(),
        }
    }
}

impl PrimaryNetwork {
    fn client(&mut self, address: SocketAddr) -> PrimaryToPrimaryClient<Channel> {
        self.clients
            .entry(address)
            .or_insert_with(|| Self::create_client(address))
            .clone()
    }

    fn create_client(address: SocketAddr) -> PrimaryToPrimaryClient<Channel> {
        // TODO use TLS
        let url = format!("http://{}", address);
        let channel = Channel::from_shared(url)
            .expect("URI should be valid")
            .connect_lazy();
        PrimaryToPrimaryClient::new(channel)
    }

    pub async fn send<T: VerifyingKey>(
        &mut self,
        address: SocketAddr,
        message: &PrimaryMessage<T>,
    ) -> CancelHandler<()> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        self.send_message(address, message).await
    }

    async fn send_message(
        &mut self,
        address: SocketAddr,
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
        addresses: Vec<SocketAddr>,
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
        address: SocketAddr,
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
        addresses: Vec<SocketAddr>,
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
        mut addresses: Vec<SocketAddr>,
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
    clients: HashMap<SocketAddr, PrimaryToWorkerClient<Channel>>,
}

impl PrimaryToWorkerNetwork {
    fn client(&mut self, address: SocketAddr) -> PrimaryToWorkerClient<Channel> {
        self.clients
            .entry(address)
            .or_insert_with(|| Self::create_client(address))
            .clone()
    }

    fn create_client(address: SocketAddr) -> PrimaryToWorkerClient<Channel> {
        // TODO use TLS
        let url = format!("http://{}", address);
        let channel = Channel::from_shared(url)
            .expect("URI should be valid")
            .connect_lazy();
        PrimaryToWorkerClient::new(channel)
    }

    pub async fn send<T: VerifyingKey>(
        &mut self,
        address: SocketAddr,
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
        addresses: Vec<SocketAddr>,
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
