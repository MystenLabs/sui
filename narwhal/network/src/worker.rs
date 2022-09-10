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
use tokio::runtime::Handle;
use tonic::{transport::Channel, Code};
use tracing::error;
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
    concurrency_limit_per_client: usize,
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
            concurrency_limit_per_client: MAX_TASK_CONCURRENCY,
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

    // used for testing non-blocking behavior
    #[cfg(test)]
    fn new_with_concurrency_limit(concurrency_limit: usize) -> Self {
        Self {
            concurrency_limit_per_client: concurrency_limit,
            ..Self::default()
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
        // TODO: Add protection for primary owned worker addresses (issue#840).
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
    ) -> () {
        let mut client = self.client(address.clone());
        self.executors
            .entry(address)
            .or_insert_with(|| {
                BoundedExecutor::new(self.concurrency_limit_per_client, Handle::current())
            })
            .try_spawn(async move {
                let _ = client.send_message(message).await;
            })
            .ok();
        self.update_metrics();
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
                    match e.code() {
                        Code::FailedPrecondition | Code::InvalidArgument => {
                            // these errors are not recoverable through retrying, see
                            // https://github.com/hyperium/tonic/blob/master/tonic/src/status.rs
                            error!("Irrecoverable network error: {e}");
                            backoff::Error::permanent(eyre::Report::from(e))
                        }
                        _ => {
                            // this returns a backoff::Error::Transient
                            // so that if tonic::Status is returned, we retry
                            Into::<backoff::Error<eyre::Report>>::into(eyre::Report::from(e))
                        }
                    }
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
                    match e.code() {
                        Code::FailedPrecondition | Code::InvalidArgument => {
                            // these errors are not recoverable through retrying, see
                            // https://github.com/hyperium/tonic/blob/master/tonic/src/status.rs
                            error!("Irrecoverable network error: {e}");
                            backoff::Error::permanent(eyre::Report::from(e))
                        }
                        _ => {
                            // this returns a backoff::Error::Transient
                            // so that if tonic::Status is returned, we retry
                            Into::<backoff::Error<eyre::Report>>::into(eyre::Report::from(e))
                        }
                    }
                })
            }
        };
        let handle = self
            .executor
            .spawn_with_retries(self.retry_config, message_send);

        CancelOnDropHandler(handle)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use types::Batch;

    use super::*;

    #[tokio::test]
    async fn unreliable_send_doesnt_block() {
        let test_concurrency_limit = 2;
        let mut p2p = WorkerNetwork::new_with_concurrency_limit(test_concurrency_limit);
        // send those messages to localhost. THey won't actually land
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();
        let serialized_msg =
            BincodeEncodedPayload::try_from(&WorkerMessage::Batch(Batch::default())).unwrap();

        let blast_a_few = async move {
            for _i in 0..test_concurrency_limit * 2 {
                let addr = addr.clone();
                let msg = serialized_msg.clone();
                p2p.unreliable_send_message(addr, msg).await
            }
        };

        // beware: if we happen to set a default connect timeout
        // (we don't at the time of writing) then the following delay needs to be smaller
        let blast_timeout = Duration::from_millis(100);
        tokio::time::timeout(blast_timeout, blast_a_few)
            .await
            .expect("The unreliable sends should all have completed instantly!");
    }
}
