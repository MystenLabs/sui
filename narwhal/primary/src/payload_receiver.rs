// Copyright(C) Facebook, Inc. and its affiliates.
use config::WorkerId;
use crypto::Digest;
use store::Store;
use tokio::sync::mpsc::Receiver;

/// Receives batches' digests of other authorities. These are only needed to verify incoming
/// headers (ie. make sure we have their payload).
pub struct PayloadReceiver {
    /// The persistent storage.
    store: Store,
    /// Receives batches' digests from the network.
    rx_workers: Receiver<(Digest, WorkerId)>,
}

impl PayloadReceiver {
    pub fn spawn(store: Store, rx_workers: Receiver<(Digest, WorkerId)>) {
        tokio::spawn(async move {
            Self { store, rx_workers }.run().await;
        });
    }

    async fn run(&mut self) {
        while let Some((digest, worker_id)) = self.rx_workers.recv().await {
            let key = [digest.as_ref(), &worker_id.to_le_bytes()].concat();
            self.store.write(key.to_vec(), Vec::default()).await;
        }
    }
}
