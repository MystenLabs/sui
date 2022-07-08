// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{BoundedExecutor, CancelHandler, RetryConfig, MAX_TASK_CONCURRENCY};
use crypto::traits::VerifyingKey;
use futures::FutureExt;
use multiaddr::Multiaddr;
use rand::{prelude::SliceRandom as _, rngs::SmallRng, SeedableRng as _};
use std::collections::HashMap;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tonic::transport::Channel;
use types::{BincodeEncodedPayload, WorkerMessage, WorkerToWorkerClient};

pub struct WorkerNetwork {
    clients: HashMap<Multiaddr, WorkerToWorkerClient<Channel>>,
    config: mysten_network::config::Config,
    retry_config: RetryConfig,
    /// Small RNG just used to shuffle nodes and randomize connections (not crypto related).
    rng: SmallRng,
    executor: BoundedExecutor,
}

impl Default for WorkerNetwork {
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
            executor: BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current()),
        }
    }
}

impl WorkerNetwork {
    fn client(&mut self, address: Multiaddr) -> WorkerToWorkerClient<Channel> {
        self.clients
            .entry(address.clone())
            .or_insert_with(|| Self::create_client(&self.config, address))
            .clone()
    }

    fn create_client(
        config: &mysten_network::config::Config,
        address: Multiaddr,
    ) -> WorkerToWorkerClient<Channel> {
        //TODO don't panic here if address isn't supported
        let channel = config.connect_lazy(&address).unwrap();
        WorkerToWorkerClient::new(channel)
    }

    pub async fn send<T: VerifyingKey>(
        &mut self,
        address: Multiaddr,
        message: &WorkerMessage<T>,
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
        let handle = self
            .executor
            .spawn(
                self.retry_config
                    .retry(move || {
                        let mut client = client.clone();
                        let message = message.clone();
                        async move { client.send_message(message).await.map_err(Into::into) }
                    })
                    .map(|response| {
                        response.expect("we retry forever so this shouldn't fail");
                    }),
            )
            .await;

        CancelHandler(handle)
    }

    pub async fn broadcast<T: VerifyingKey>(
        &mut self,
        addresses: Vec<Multiaddr>,
        message: &WorkerMessage<T>,
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
        message: &WorkerMessage<T>,
    ) -> JoinHandle<()> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        self.unreliable_send_message(address, message).await
    }

    pub async fn unreliable_send_message<T: Into<BincodeEncodedPayload>>(
        &mut self,
        address: Multiaddr,
        message: T,
    ) -> JoinHandle<()> {
        let message = message.into();
        let mut client = self.client(address);
        self.executor
            .spawn(async move {
                let _ = client.send_message(message).await;
            })
            .await
    }

    /// Pick a few addresses at random (specified by `nodes`) and try (best-effort) to send the
    /// message only to them. This is useful to pick nodes with whom to sync.
    pub async fn lucky_broadcast<T: VerifyingKey>(
        &mut self,
        mut addresses: Vec<Multiaddr>,
        message: &WorkerMessage<T>,
        nodes: usize,
    ) -> Vec<JoinHandle<()>> {
        addresses.shuffle(&mut self.rng);
        addresses.truncate(nodes);
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let mut handlers = Vec::new();
        for address in addresses {
            let handle = {
                let message = message.clone();
                self.unreliable_send_message(address, message).await
            };
            handlers.push(handle);
        }
        handlers
    }
}
