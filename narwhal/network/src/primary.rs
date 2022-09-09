// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    metrics::{Metrics, PrimaryNetworkMetrics},
    traits::{BaseNetwork, Lucky, ReliableNetwork2, UnreliableNetwork, UnreliableNetwork2},
    BoundedExecutor, CancelOnDropHandler, MessageResult, ReliableNetwork, RetryConfig,
    MAX_TASK_CONCURRENCY,
};
use anemo::PeerId;
use async_trait::async_trait;
use crypto::NetworkPublicKey;
use multiaddr::Multiaddr;
use rand::{rngs::SmallRng, SeedableRng as _};
use std::collections::HashMap;
use tokio::{runtime::Handle, task::JoinHandle};
use tonic::transport::Channel;
use tracing::error;
use types::{
    BincodeEncodedPayload, PrimaryMessage, PrimaryToPrimaryClient, PrimaryToWorkerClient,
    PrimaryWorkerMessage, WorkerMessage, WorkerToWorkerClient,
};

fn default_executor() -> BoundedExecutor {
    BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current())
}

pub struct PrimaryToWorkerNetwork {
    clients: HashMap<Multiaddr, PrimaryToWorkerClient<Channel>>,
    config: mysten_network::config::Config,
    executor: BoundedExecutor,
    retry_config: RetryConfig,
    metrics: Option<Metrics<PrimaryNetworkMetrics>>,
}

impl PrimaryToWorkerNetwork {
    pub fn new(metrics: Metrics<PrimaryNetworkMetrics>) -> Self {
        Self {
            metrics: Some(Metrics::from(metrics, "primary_to_worker".to_string())),
            ..Default::default()
        }
    }

    // used for testing non-blocking behavior
    #[cfg(test)]
    fn new_with_concurrency_limit(concurrency_limit: usize) -> Self {
        Self {
            executor: BoundedExecutor::new(concurrency_limit, Handle::current()),
            ..Self::default()
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
            retry_config: RetryConfig::default(),
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
    ) -> () {
        let mut client = self.client(address);
        self.executor
            .try_spawn(async move {
                let _ = client.send_message(message).await;
            })
            .ok();

        self.update_metrics();
    }
}

#[async_trait]
impl ReliableNetwork for PrimaryToWorkerNetwork {
    // Safety
    // Since this spawns an unbounded task, this should be called in a time-restricted fashion.
    // Here the callers are [`PrimaryNetwork::broadcast`] and [`PrimaryNetwork::send`],
    // at respectively N and K calls per round.
    //  (where N is the number of primaries, K the number of workers for this primary)
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
                        tonic::Code::FailedPrecondition | tonic::Code::InvalidArgument => {
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

        self.update_metrics();

        CancelOnDropHandler(handle)
    }
}

pub struct P2pNetwork {
    network: anemo::Network,
    retry_config: RetryConfig,
    /// Small RNG just used to shuffle nodes and randomize connections (not crypto related).
    rng: SmallRng,
    // One bounded executor per address
    executors: HashMap<anemo::PeerId, BoundedExecutor>,
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

    async fn unreliable_send<F, Fut, O>(&mut self, peer: NetworkPublicKey, f: F) -> JoinHandle<()>
    where
        F: FnOnce(anemo::Peer) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = O> + Send,
    {
        let network = self.network.clone();
        let peer_id = PeerId(peer.0.to_bytes());
        self.executors
            .entry(peer_id)
            .or_insert_with(default_executor)
            .spawn(async move {
                if let Some(peer) = network.peer(peer_id) {
                    let _ = f(peer).await;
                }
            })
            .await
    }

    async fn send<F, Fut>(
        &mut self,
        peer: NetworkPublicKey,
        f: F,
    ) -> CancelOnDropHandler<anyhow::Result<anemo::Response<()>>>
    where
        F: Fn(anemo::Peer) -> Fut + Send + Sync + 'static + Clone,
        Fut: std::future::Future<Output = Result<anemo::Response<()>, anemo::rpc::Status>> + Send,
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
impl UnreliableNetwork2<PrimaryMessage> for P2pNetwork {
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &PrimaryMessage,
    ) -> JoinHandle<()> {
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
impl ReliableNetwork2<PrimaryMessage> for P2pNetwork {
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &PrimaryMessage,
    ) -> CancelOnDropHandler<anyhow::Result<anemo::Response<()>>> {
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
// Worker-to-Worker
//

#[async_trait]
impl UnreliableNetwork2<WorkerMessage> for P2pNetwork {
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerMessage,
    ) -> JoinHandle<()> {
        let message = message.to_owned();
        let f =
            move |peer| async move { WorkerToWorkerClient::new(peer).send_message(message).await };
        self.unreliable_send(peer, f).await
    }
}

#[async_trait]
impl ReliableNetwork2<WorkerMessage> for P2pNetwork {
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerMessage,
    ) -> CancelOnDropHandler<anyhow::Result<anemo::Response<()>>> {
        let message = message.to_owned();
        let f = move |peer| {
            let message = message.clone();
            async move { WorkerToWorkerClient::new(peer).send_message(message).await }
        };

        self.send(peer, f).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn unreliable_send_doesnt_block() {
        let test_concurrency_limit = 2;
        let mut p2p = PrimaryToWorkerNetwork::new_with_concurrency_limit(test_concurrency_limit);
        // send those messages to localhost. THey won't actually land
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();
        let serialized_msg =
            BincodeEncodedPayload::try_from(&PrimaryWorkerMessage::Cleanup(42)).unwrap();

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
