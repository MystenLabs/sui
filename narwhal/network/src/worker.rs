// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    traits::{BaseNetwork, ReliableNetwork},
    BoundedExecutor, CancelOnDropHandler, MessageResult, RetryConfig, MAX_TASK_CONCURRENCY,
};
use async_trait::async_trait;
use multiaddr::Multiaddr;
use tokio::runtime::Handle;
use tonic::{transport::Channel, Code};
use tracing::error;
use types::{BincodeEncodedPayload, WorkerPrimaryMessage, WorkerToPrimaryClient};

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
