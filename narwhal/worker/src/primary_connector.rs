// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use primary::WorkerPrimaryMessage;
use std::net::SocketAddr;
use tokio::sync::mpsc::Receiver;
use tonic::transport::Channel;
use types::{BincodeEncodedPayload, WorkerToPrimaryClient};

// Send batches' digests to the primary.
pub struct PrimaryConnector {
    /// The primary network address.
    _primary_address: SocketAddr,
    /// Input channel to receive the messages to send to the primary.
    rx_digest: Receiver<WorkerPrimaryMessage>,
    /// A network sender to send the batches' digests to the primary.
    primary_client: PrimaryClient,
}

impl PrimaryConnector {
    pub fn spawn(primary_address: SocketAddr, rx_digest: Receiver<WorkerPrimaryMessage>) {
        tokio::spawn(async move {
            Self {
                _primary_address: primary_address,
                rx_digest,
                primary_client: PrimaryClient::new(primary_address),
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        while let Some(digest) = self.rx_digest.recv().await {
            // Send the digest through the network.
            // We don't care about the error
            let _ = self.primary_client.send(&digest).await;
        }
    }
}

#[derive(Clone)]
pub struct PrimaryClient {
    /// The primary network address.
    _primary_address: SocketAddr,
    client: WorkerToPrimaryClient<Channel>,
}

impl PrimaryClient {
    pub fn new(address: SocketAddr) -> Self {
        // TODO maybe use TLS
        let url = format!("http://{}", address);
        let channel = Channel::from_shared(url)
            .expect("URI should be valid")
            .connect_lazy();
        let client = WorkerToPrimaryClient::new(channel);

        Self {
            _primary_address: address,
            client,
        }
    }

    pub async fn send(&mut self, message: &WorkerPrimaryMessage) -> anyhow::Result<()> {
        let message = BincodeEncodedPayload::try_from(message)?;
        self.client
            .send_message(message)
            .await
            .map_err(Into::into)
            .map(|_| ())
    }
}
