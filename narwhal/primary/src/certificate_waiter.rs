// Copyright(C) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    error::{DagError, DagResult},
    messages::Certificate,
};
use crypto::Digest;
use futures::{
    future::try_join_all,
    stream::{futures_unordered::FuturesUnordered, StreamExt as _},
};
use log::error;
use store::Store;
use tokio::sync::mpsc::{Receiver, Sender};

/// Waits to receive all the ancestors of a certificate before looping it back to the `Core`
/// for further processing.
pub struct CertificateWaiter {
    /// The persistent storage.
    store: Store<Digest, Certificate>,
    /// Receives sync commands from the `Synchronizer`.
    rx_synchronizer: Receiver<Certificate>,
    /// Loops back to the core certificates for which we got all parents.
    tx_core: Sender<Certificate>,
}

impl CertificateWaiter {
    pub fn spawn(
        store: Store<Digest, Certificate>,
        rx_synchronizer: Receiver<Certificate>,
        tx_core: Sender<Certificate>,
    ) {
        tokio::spawn(async move {
            Self {
                store,
                rx_synchronizer,
                tx_core,
            }
            .run()
            .await
        });
    }

    /// Helper function. It waits for particular data to become available in the storage
    /// and then delivers the specified header.
    async fn waiter(
        // TODO: this signature is just here to avoid thinking about lifetimes, fix it
        mut missing: Vec<(Digest, Store<Digest, Certificate>)>,
        deliver: Certificate,
    ) -> DagResult<Certificate> {
        let waiting: Vec<_> = missing
            .iter_mut()
            .map(|(x, y)| y.notify_read(x.clone()))
            .collect();

        try_join_all(waiting)
            .await
            .map(|_| deliver)
            .map_err(DagError::from)
    }

    async fn run(&mut self) {
        let mut waiting = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(certificate) = self.rx_synchronizer.recv() => {
                    // Add the certificate to the waiter pool. The waiter will return it to us
                    // when all its parents are in the store.
                    let wait_for = certificate
                        .header
                        .parents
                        .iter()
                        .cloned()
                        .map(|x| (x, self.store.clone()))
                        .collect();
                    let fut = Self::waiter(wait_for, certificate);
                    waiting.push(fut);
                }
                Some(result) = waiting.next() => match result {
                    Ok(certificate) => {
                        self.tx_core.send(certificate).await.expect("Failed to send certificate");
                    },
                    Err(e) => {
                        error!("{}", e);
                        panic!("Storage failure: killing node.");
                    }
                },
            }
        }
    }
}
