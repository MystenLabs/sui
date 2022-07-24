// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::WorkerId;
use crypto::traits::VerifyingKey;
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tracing::error;
use types::{
    error::DagError, serialized_batch_digest, BatchDigest, ReconfigureNotification,
    SerializedBatchMessage, WorkerPrimaryMessage,
};

#[cfg(test)]
#[path = "tests/processor_tests.rs"]
pub mod processor_tests;

/// Hashes and stores batches, it then outputs the batch's digest.
pub struct Processor;

impl Processor {
    pub fn spawn<PublicKey: VerifyingKey>(
        // Our worker's id.
        id: WorkerId,
        // The persistent storage.
        store: Store<BatchDigest, SerializedBatchMessage>,
        // Receive reconfiguration signals.
        mut rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        // Input channel to receive batches.
        mut rx_batch: Receiver<SerializedBatchMessage>,
        // Output channel to send out batches' digests.
        tx_digest: Sender<WorkerPrimaryMessage<PublicKey>>,
        // Whether we are processing our own batches or the batches of other nodes.
        own_digest: bool,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(batch) = rx_batch.recv() => {
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
                                if tx_digest
                                    .send(message)
                                    .await
                                    .is_err() {
                                    tracing::debug!("{}", DagError::ShuttingDown);
                                };
                            }
                            Err(error) => {
                                error!("Received invalid batch, serialization failure: {error}");
                            }
                        }
                    },

                    // Trigger reconfigure.
                    result = rx_reconfigure.changed() => {
                        result.expect("Committee channel dropped");
                        let message = rx_reconfigure.borrow().clone();
                        if let ReconfigureNotification::Shutdown = message {
                            return;
                        }
                    }
                }
            }
        })
    }
}
