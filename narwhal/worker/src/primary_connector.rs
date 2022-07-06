// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::Committee;
use crypto::traits::VerifyingKey;
use multiaddr::Multiaddr;
use primary::WorkerPrimaryMessage;
use tokio::{
    sync::{mpsc::Receiver, watch},
    task::JoinHandle,
};
use tonic::transport::Channel;
use types::{BincodeEncodedPayload, Reconfigure, WorkerToPrimaryClient};

// Send batches' digests to the primary.
pub struct PrimaryConnector<PublicKey: VerifyingKey> {
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<Reconfigure<PublicKey>>,
    /// Input channel to receive the messages to send to the primary.
    rx_digest: Receiver<WorkerPrimaryMessage>,
    /// A network sender to send the batches' digests to the primary.
    primary_client: PrimaryClient,
}

impl<PublicKey: VerifyingKey> PrimaryConnector<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        rx_reconfigure: watch::Receiver<Reconfigure<PublicKey>>,
        rx_digest: Receiver<WorkerPrimaryMessage>,
    ) -> JoinHandle<()> {
        let address = committee
            .primary(&name)
            .expect("Our public key is not in the committee")
            .worker_to_primary;
        tokio::spawn(async move {
            Self {
                rx_reconfigure,
                rx_digest,
                primary_client: PrimaryClient::new(address),
            }
            .run()
            .await;
        })
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Send the digest through the network.
                Some(digest) = self.rx_digest.recv() => {
                    // We don't care about the error
                    let _ = self.primary_client.send(&digest).await;
                },
                // Trigger reconfigure.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    if let Reconfigure::Shutdown(_token) = message {
                        return;
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct PrimaryClient {
    /// The primary network address.
    _primary_address: Multiaddr,
    client: WorkerToPrimaryClient<Channel>,
}

impl PrimaryClient {
    pub fn new(address: Multiaddr) -> Self {
        //TODO don't panic here if address isn't supported
        let config = mysten_network::config::Config::new();
        let channel = config.connect_lazy(&address).unwrap();
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
