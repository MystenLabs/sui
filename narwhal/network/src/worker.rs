// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{BoundedExecutor, CancelHandler, MessageResult, RetryConfig, MAX_TASK_CONCURRENCY};
use crypto::traits::VerifyingKey;
use multiaddr::Multiaddr;
use rand::{prelude::SliceRandom as _, rngs::SmallRng, SeedableRng as _};
use std::collections::HashMap;
use tokio::{runtime::Handle, task::JoinHandle};
use tonic::transport::Channel;
use types::{
    BincodeEncodedPayload, WorkerMessage, WorkerPrimaryMessage, WorkerToPrimaryClient,
    WorkerToWorkerClient,
};

pub struct WorkerNetwork {
    clients: HashMap<Multiaddr, WorkerToWorkerClient<Channel>>,
    config: mysten_network::config::Config,
    retry_config: RetryConfig,
    /// Small RNG just used to shuffle nodes and randomize connections (not crypto related).
    rng: SmallRng,
    executors: HashMap<Multiaddr, BoundedExecutor>,
}

fn default_executor() -> BoundedExecutor {
    BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current())
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
            executors: HashMap::new(),
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
    ) -> CancelHandler<MessageResult> {
        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        self.send_message(address, message).await
    }

    // Safety
    // Since this spawns an unbounded task, this should be called in a time-restricted fashion.
    // Here the callers are [`WorkerNetwork::broadcast`] and [`WorkerNetwork::send`],
    // at respectively N and K calls per round.
    //  (where N is the number of validators, the K is for the number of batches to be reported to the primary)
    // See the TODO on spawn_with_retries for lifting this restriction.
    async fn send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> CancelHandler<MessageResult> {
        let client = self.client(address.clone());

        let message_send = move || {
            let mut client = client.clone();
            let message = message.clone();

            async move {
                client.send_message(message).await.map_err(|e| {
                    // this returns a backoff::Error::Transient
                    // so that if tonic::Status is returned, we retry
                    Into::<backoff::Error<anyhow::Error>>::into(anyhow::Error::from(e))
                })
            }
        };

        let handle = self
            .executors
            .entry(address)
            .or_insert_with(default_executor)
            .spawn_with_retries(self.retry_config, message_send);

        CancelHandler(handle)
    }

    pub async fn broadcast<T: VerifyingKey>(
        &mut self,
        addresses: Vec<Multiaddr>,
        message: &WorkerMessage<T>,
    ) -> Vec<CancelHandler<MessageResult>> {
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
        let mut client = self.client(address.clone());
        self.executors
            .entry(address)
            .or_insert_with(default_executor)
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

pub struct WorkerToPrimaryNetwork {
    address: Option<Multiaddr>,
    client: Option<WorkerToPrimaryClient<Channel>>,
    config: mysten_network::config::Config,
    retry_config: RetryConfig,
    executor: BoundedExecutor,
}

impl Default for WorkerToPrimaryNetwork {
    fn default() -> Self {
        let retry_config = RetryConfig {
            // Retry for forever
            retrying_max_elapsed_time: None,
            ..Default::default()
        };

        Self {
            address: None,
            client: Default::default(),
            config: Default::default(),
            retry_config,
            // Note that this does not strictly break the primitive that BoundedExecutor is per address because
            // this network sender only transmits to a single address.
            executor: BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current()),
        }
    }
}

impl WorkerToPrimaryNetwork {
    // Safety
    // Since this spawns an unbounded task, this should be called in a time-restricted fashion.
    // Here the callers are [`WorkerToPrimaryNetwork::send`].
    // See the TODO on spawn_with_retries for lifting this restriction.
    pub async fn send<PublicKey: VerifyingKey>(
        &mut self,
        address: Multiaddr,
        message: &WorkerPrimaryMessage<PublicKey>,
    ) -> CancelHandler<MessageResult> {
        let new_client = match &self.address {
            None => true,
            Some(x) if x != &address => true,
            _ => false,
        };
        if new_client {
            let channel = self.config.connect_lazy(&address).unwrap();
            self.client = Some(WorkerToPrimaryClient::new(channel));
            self.address = Some(address);
        }

        let message =
            BincodeEncodedPayload::try_from(message).expect("Failed to serialize payload");
        let client = self.client.as_mut().unwrap().clone();

        let message_send = move || {
            let mut client = client.clone();
            let message = message.clone();

            async move {
                client.send_message(message).await.map_err(|e| {
                    // this returns a backoff::Error::Transient
                    // so that if tonic::Status is returned, we retry
                    Into::<backoff::Error<anyhow::Error>>::into(anyhow::Error::from(e))
                })
            }
        };
        let handle = self
            .executor
            .spawn_with_retries(self.retry_config, message_send);

        CancelHandler(handle)
    }
}
