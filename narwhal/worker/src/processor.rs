// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::WorkerId;
use primary::WorkerPrimaryMessage;
use store::Store;
use tokio::{
    sync::mpsc::{Receiver, Sender},
    task::JoinHandle,
};
use tracing::error;
use types::{serialized_batch_digest, BatchDigest, SerializedBatchMessage};

#[cfg(test)]
#[path = "tests/processor_tests.rs"]
pub mod processor_tests;

/// Hashes and stores batches, it then outputs the batch's digest.
pub struct Processor;

impl Processor {
    pub fn spawn(
        // Our worker's id.
        id: WorkerId,
        // The persistent storage.
        store: Store<BatchDigest, SerializedBatchMessage>,
        // Input channel to receive batches.
        mut rx_batch: Receiver<SerializedBatchMessage>,
        // Output channel to send out batches' digests.
        tx_digest: Sender<WorkerPrimaryMessage>,
        // Whether we are processing our own batches or the batches of other nodes.
        own_digest: bool,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(batch) = rx_batch.recv().await {
                // Hash the batch.
                let res_digest = serialized_batch_digest(&batch);

                match res_digest {
                    Ok(digest) => {
                        // Store the batch.
                        store.write(digest, batch).await;

                        // Deliver the batch's digest.
                        let message = match own_digest {
                            true => WorkerPrimaryMessage::OurBatch(digest, id),
                            false => WorkerPrimaryMessage::OthersBatch(digest, id),
                        };
                        tx_digest
                            .send(message)
                            .await
                            .expect("Failed to send digest");
                    }
                    Err(error) => {
                        error!("Received invalid batch, serialization failure: {error}");
                    }
                }
            }
        })
    }
}
