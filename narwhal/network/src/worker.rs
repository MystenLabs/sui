// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    metrics::{Metrics, WorkerNetworkMetrics},
    traits::{BaseNetwork, LuckyNetwork, ReliableNetwork, UnreliableNetwork},
    BoundedExecutor, CancelOnDropHandler, MessageResult, RetryConfig, MAX_TASK_CONCURRENCY,
};
use async_trait::async_trait;
use multiaddr::Multiaddr;
use rand::{rngs::SmallRng, SeedableRng as _};
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
    metrics: Option<Metrics<WorkerNetworkMetrics>>,
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
            metrics: None,
        }
    }
}

impl WorkerNetwork {
    pub fn new(metrics: Metrics<WorkerNetworkMetrics>) -> Self {
        Self {
            metrics: Some(Metrics::from(metrics, "worker".to_string())),
            ..Default::default()
        }
    }

    fn update_metrics(&self) {
        if let Some(m) = &self.metrics {
            for (addr, executor) in &self.executors {
                m.set_network_available_tasks(
                    executor.available_capacity() as i64,
                    Some(addr.to_string()),
                );
            }
        }
    }

    pub fn cleanup<'a, I>(&mut self, to_remove: I)
    where
        I: IntoIterator<Item = &'a Multiaddr>,
    {
        for address in to_remove {
            self.clients.remove(address);
        }
    }
}

impl BaseNetwork for WorkerNetwork {
    type Client = WorkerToWorkerClient<Channel>;
    type Message = WorkerMessage;

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
}

#[async_trait]
impl UnreliableNetwork for WorkerNetwork {
    async fn unreliable_send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> JoinHandle<()> {
        let mut client = self.client(address.clone());
        let handler = self
            .executors
            .entry(address)
            .or_insert_with(default_executor)
            .spawn(async move {
                let _ = client.send_message(message).await;
            })
            .await;
        self.update_metrics();

        handler
    }
}

#[async_trait]
impl LuckyNetwork for WorkerNetwork {
    fn rng(&mut self) -> &mut SmallRng {
        &mut self.rng
    }
}

#[async_trait]
impl ReliableNetwork for WorkerNetwork {
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
    ) -> CancelOnDropHandler<MessageResult> {
        let client = self.client(address.clone());

        let message_send = move || {
            let mut client = client.clone();
            let message = message.clone();

            async move {
                client.send_message(message).await.map_err(|e| {
                    // this returns a backoff::Error::Transient
                    // so that if tonic::Status is returned, we retry
                    Into::<backoff::Error<eyre::Report>>::into(eyre::Report::from(e))
                })
            }
        };

        let handle = self
            .executors
            .entry(address)
            .or_insert_with(default_executor)
            .spawn_with_retries(self.retry_config, message_send);

        self.update_metrics();

        CancelOnDropHandler(handle)
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

impl BaseNetwork for WorkerToPrimaryNetwork {
    type Client = WorkerToPrimaryClient<Channel>;
    type Message = WorkerPrimaryMessage;

    fn client(&mut self, address: Multiaddr) -> Self::Client {
        match (self.address.as_ref(), self.client.as_ref()) {
            (Some(addr), Some(client)) if *addr == address => client.clone(),
            (_, _) => {
                let client = Self::create_client(&self.config, address.clone());
                self.client = Some(client.clone());
                self.address = Some(address);
                client
            }
        }
    }

    fn create_client(config: &mysten_network::config::Config, address: Multiaddr) -> Self::Client {
        //TODO don't panic here if address isn't supported
        let channel = config.connect_lazy(&address).unwrap();
        WorkerToPrimaryClient::new(channel)
    }
}

#[async_trait]
impl ReliableNetwork for WorkerToPrimaryNetwork {
    // Safety
    // Since this spawns an unbounded task, this should be called in a time-restricted fashion.
    // Here the callers are [`WorkerToPrimaryNetwork::send`].
    // See the TODO on spawn_with_retries for lifting this restriction.
    async fn send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> CancelOnDropHandler<MessageResult> {
        let client = self.client(address);
        let message_send = move || {
            let mut client = client.clone();
            let message = message.clone();

            async move {
                client.send_message(message).await.map_err(|e| {
                    // this returns a backoff::Error::Transient
                    // so that if tonic::Status is returned, we retry
                    Into::<backoff::Error<eyre::Report>>::into(eyre::Report::from(e))
                })
            }
        };
        let handle = self
            .executor
            .spawn_with_retries(self.retry_config, message_send);

        CancelOnDropHandler(handle)
    }
}
