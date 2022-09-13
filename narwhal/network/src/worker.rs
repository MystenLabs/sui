// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    traits::{ReliableNetwork2, UnreliableNetwork2},
    BoundedExecutor, CancelOnDropHandler, RetryConfig, MAX_TASK_CONCURRENCY,
};
use anemo::PeerId;
use async_trait::async_trait;
use crypto::{NetworkKeyPair, NetworkPublicKey};
use fastcrypto::traits::KeyPair as _;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::{runtime::Handle, task::JoinHandle};

use types::{WorkerPrimaryMessage, WorkerToPrimaryClient};

pub struct WorkerToPrimaryNetwork {
    network: anemo::Network,
    retry_config: RetryConfig,
    executor: BoundedExecutor,
}

impl WorkerToPrimaryNetwork {
    pub fn new(network: anemo::Network) -> Self {
        let retry_config = RetryConfig {
            // Retry forever.
            retrying_max_elapsed_time: None,
            ..Default::default()
        };

        Self {
            network,
            retry_config,
            // Note that this does not strictly break the primitive that BoundedExecutor is per address because
            // this network sender only transmits to a single address.
            executor: BoundedExecutor::new(MAX_TASK_CONCURRENCY, Handle::current()),
        }
    }

    // Creates a new single-use anemo::Network to connect outbound to a single
    // address. This is for tests and should not be used from worker code.
    pub async fn new_for_single_address(
        name: NetworkPublicKey,
        address: anemo::types::Address,
    ) -> Self {
        let routes = anemo::Router::new();
        let network = anemo::Network::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))
            .server_name("narwhal")
            .private_key(
                NetworkKeyPair::generate(&mut rand::rngs::OsRng)
                    .private()
                    .0
                    .to_bytes(),
            )
            .start(routes)
            .unwrap();
        network
            .connect_with_peer_id(address, anemo::PeerId(name.0.to_bytes()))
            .await
            .unwrap();
        Self::new(network)
    }
}

#[async_trait]
impl UnreliableNetwork2<WorkerPrimaryMessage> for WorkerToPrimaryNetwork {
    async fn unreliable_send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerPrimaryMessage,
    ) -> JoinHandle<()> {
        let network = self.network.clone();
        let peer_id = PeerId(peer.0.to_bytes());
        let message = message.to_owned();
        self.executor
            .spawn(async move {
                if let Some(peer) = network.peer(peer_id) {
                    let _ = WorkerToPrimaryClient::new(peer).send_message(message).await;
                }
            })
            .await
    }
}

#[async_trait]
impl ReliableNetwork2<WorkerPrimaryMessage> for WorkerToPrimaryNetwork {
    async fn send(
        &mut self,
        peer: NetworkPublicKey,
        message: &WorkerPrimaryMessage,
    ) -> CancelOnDropHandler<anyhow::Result<anemo::Response<()>>> {
        let network = self.network.clone();
        let peer_id = PeerId(peer.0.to_bytes());
        let message = message.to_owned();
        let message_send = move || {
            let network = network.clone();
            let message = message.clone();

            async move {
                if let Some(peer) = network.peer(peer_id) {
                    WorkerToPrimaryClient::new(peer)
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
            .executor
            .spawn_with_retries(self.retry_config, message_send);

        CancelOnDropHandler(handle)
    }
}
