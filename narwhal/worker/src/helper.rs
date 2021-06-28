// Copyright(C) Facebook, Inc. and its affiliates.
use bytes::Bytes;
use config::{Committee, WorkerId};
use crypto::{Digest, PublicKey};
use log::{error, warn};
use network::SimpleSender;
use store::Store;
use tokio::sync::mpsc::Receiver;

#[cfg(test)]
#[path = "tests/helper_tests.rs"]
pub mod helper_tests;

/// A task dedicated to help other authorities by replying to their batch requests.
pub struct Helper {
    /// The id of this worker.
    id: WorkerId,
    /// The committee information.
    committee: Committee,
    /// The persistent storage.
    store: Store,
    /// Input channel to receive batch requests.
    rx_request: Receiver<(Vec<Digest>, PublicKey)>,
    /// A network sender to send the batches to the other workers.
    network: SimpleSender,
}

impl Helper {
    pub fn spawn(
        id: WorkerId,
        committee: Committee,
        store: Store,
        rx_request: Receiver<(Vec<Digest>, PublicKey)>,
    ) {
        tokio::spawn(async move {
            Self {
                id,
                committee,
                store,
                rx_request,
                network: SimpleSender::new(),
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        while let Some((digests, origin)) = self.rx_request.recv().await {
            // TODO [issue #7]: Do some accounting to prevent bad nodes from monopolizing our resources.

            // get the requestors address.
            let address = match self.committee.worker(&origin, &self.id) {
                Ok(x) => x.worker_to_worker,
                Err(e) => {
                    warn!("Unexpected batch request: {}", e);
                    continue;
                }
            };

            // Reply to the request (the best we can).
            for digest in digests {
                match self.store.read(digest.to_vec()).await {
                    Ok(Some(data)) => self.network.send(address, Bytes::from(data)).await,
                    Ok(None) => (),
                    Err(e) => error!("{}", e),
                }
            }
        }
    }
}
