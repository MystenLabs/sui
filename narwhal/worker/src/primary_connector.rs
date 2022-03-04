// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::worker::SerializedWorkerPrimaryMessage;
use bytes::Bytes;
use network::SimpleSender;
use std::net::SocketAddr;
use tokio::sync::mpsc::Receiver;

// Send batches' digests to the primary.
pub struct PrimaryConnector {
    /// The primary network address.
    primary_address: SocketAddr,
    /// Input channel to receive the messages to send to the primary.
    rx_digest: Receiver<SerializedWorkerPrimaryMessage>,
    /// A network sender to send the batches' digests to the primary.
    network: SimpleSender,
}

impl PrimaryConnector {
    pub fn spawn(primary_address: SocketAddr, rx_digest: Receiver<SerializedWorkerPrimaryMessage>) {
        tokio::spawn(async move {
            Self {
                primary_address,
                rx_digest,
                network: SimpleSender::new(),
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        while let Some(digest) = self.rx_digest.recv().await {
            // Send the digest through the network.
            self.network
                .send(self.primary_address, Bytes::from(digest))
                .await;
        }
    }
}
