// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    metrics::{Metrics, PrimaryNetworkMetrics},
    traits::{BaseNetwork, Lucky, ReliableNetwork2, UnreliableNetwork, UnreliableNetwork2},
    BoundedExecutor, CancelOnDropHandler, RetryConfig, MAX_TASK_CONCURRENCY,
};
use anemo::PeerId;
use async_trait::async_trait;
use crypto::PublicKey;
use multiaddr::Multiaddr;
use rand::{rngs::SmallRng, SeedableRng as _};
use std::collections::HashMap;
use tokio::{runtime::Handle, task::JoinHandle};
use tonic::transport::Channel;
use types::{
    BincodeEncodedPayload, PrimaryMessage, PrimaryToPrimaryClient, PrimaryToWorkerClient,
    PrimaryWorkerMessage,
};

fn default_executor() -> BoundedExecutor {
    BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current())
}

pub struct PrimaryToWorkerNetwork {
    clients: HashMap<Multiaddr, PrimaryToWorkerClient<Channel>>,
    config: mysten_network::config::Config,
    executor: BoundedExecutor,
    metrics: Option<Metrics<PrimaryNetworkMetrics>>,
}

impl PrimaryToWorkerNetwork {
    pub fn new(metrics: Metrics<PrimaryNetworkMetrics>) -> Self {
        Self {
            metrics: Some(Metrics::from(metrics, "primary_to_worker".to_string())),
            ..Default::default()
        }
    }

    fn update_metrics(&self) {
        if let Some(m) = &self.metrics {
            m.set_network_available_tasks(self.executor.available_capacity() as i64, None);
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

impl Default for PrimaryToWorkerNetwork {
    fn default() -> Self {
        Self {
            clients: Default::default(),
            config: Default::default(),
            executor: BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current()),
            metrics: None,
        }
    }
}

impl BaseNetwork for PrimaryToWorkerNetwork {
    type Client = PrimaryToWorkerClient<Channel>;
    type Message = PrimaryWorkerMessage;

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
}

#[async_trait]
impl UnreliableNetwork for PrimaryToWorkerNetwork {
    async fn unreliable_send_message(
        &mut self,
        address: Multiaddr,
        message: BincodeEncodedPayload,
    ) -> JoinHandle<()> {
        let mut client = self.client(address.clone());
        let handler = self
            .executor
            .spawn(async move {
                let _ = client.send_message(message).await;
            })
            .await;

        self.update_metrics();

        handler
    }
}

pub struct PrimaryNetwork {
    network: anemo::Network,
    retry_config: RetryConfig,
    /// Small RNG just used to shuffle nodes and randomize connections (not crypto related).
    rng: SmallRng,
    // One bounded executor per address
    executors: HashMap<anemo::PeerId, BoundedExecutor>,
}

impl PrimaryNetwork {
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
}

#[async_trait]
impl UnreliableNetwork2<PrimaryMessage> for PrimaryNetwork {
    async fn unreliable_send(
        &mut self,
        peer: PublicKey,
        message: &PrimaryMessage,
    ) -> JoinHandle<()> {
        let network = self.network.clone();
        let peer_id = PeerId(peer.0.to_bytes());
        let message = message.to_owned();
        self.executors
            .entry(peer_id)
            .or_insert_with(default_executor)
            .spawn(async move {
                if let Some(peer) = network.peer(peer_id) {
                    let _ = PrimaryToPrimaryClient::new(peer)
                        .send_message(message)
                        .await;
                }
            })
            .await
    }
}

impl Lucky for PrimaryNetwork {
    fn rng(&mut self) -> &mut SmallRng {
        &mut self.rng
    }
}

#[async_trait]
impl ReliableNetwork2<PrimaryMessage> for PrimaryNetwork {
    async fn send(
        &mut self,
        peer: PublicKey,
        message: &PrimaryMessage,
    ) -> CancelOnDropHandler<anyhow::Result<anemo::Response<()>>> {
        // Safety
        // Since this spawns an unbounded task, this should be called in a time-restricted fashion.
        // Here the callers are [`PrimaryNetwork::broadcast`] and [`PrimaryNetwork::send`],
        // at respectively N and K calls per round.
        //  (where N is the number of primaries, K the number of workers for this primary)
        // See the TODO on spawn_with_retries for lifting this restriction.

        let network = self.network.clone();
        let peer_id = PeerId(peer.0.to_bytes());
        let message = message.to_owned();
        let message_send = move || {
            let network = network.clone();
            let message = message.clone();

            async move {
                if let Some(peer) = network.peer(peer_id) {
                    PrimaryToPrimaryClient::new(peer)
                        .send_message(message)
                        .await
                        .map_err(|e| {
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
