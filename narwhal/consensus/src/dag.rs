// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crypto::traits::VerifyingKey;
use primary::Certificate;
use tokio::{sync::mpsc::Receiver, task::JoinHandle};
use tracing::debug;

/// Dag represents the pure dag that is constructed
/// by the certificate of each round without any
/// consensus running on top of it.
pub struct Dag<PublicKey: VerifyingKey> {
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: Receiver<Certificate<PublicKey>>,
}

impl<PublicKey: VerifyingKey> Dag<PublicKey> {
    pub fn spawn(rx_primary: Receiver<Certificate<PublicKey>>) -> JoinHandle<()> {
        tokio::spawn(async move { Self { rx_primary }.run().await })
    }

    async fn run(&mut self) {
        // at the moment just receive the certificate and throw away
        while self.rx_primary.recv().await.is_some() {
            debug!("Received certificate, will ignore");
        }
    }
}
