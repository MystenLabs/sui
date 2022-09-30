// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    errors::SubscriberResult, metrics::ExecutorMetrics, SubscriberError,
    SubscriberError::PayloadRetrieveError,
};
use backoff::{Error, ExponentialBackoff};
use consensus::ConsensusOutput;
use fastcrypto::Hash;
use primary::BlockCommand;
use std::{sync::Arc, time::Duration};
use store::Store;
use tokio::{
    sync::{oneshot, watch},
    task::JoinHandle,
};
use tracing::{debug_span, error, instrument, Instrument};
use types::{metered_channel, Batch, BatchDigest, CertificateDigest, ReconfigureNotification};

#[cfg(test)]
#[path = "tests/subscriber_tests.rs"]
pub mod subscriber_tests;

/// The `Subscriber` receives certificates sequenced by the consensus and waits until the
/// downloaded all the transactions references by the certificates; it then
/// forward the certificates to the Executor Core.
pub struct Subscriber {
    /// The temporary storage holding all transactions' data (that may be too big to hold in memory).
    store: Store<(CertificateDigest, BatchDigest), Batch>,
    /// Receive reconfiguration updates.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// A channel to receive consensus messages.
    rx_consensus: metered_channel::Receiver<ConsensusOutput>,
    /// A channel to send the complete and ordered list of consensus outputs to the executor. This
    /// channel is used once all transactions data are downloaded.
    tx_executor: metered_channel::Sender<ConsensusOutput>,
    // A channel to send commands to the block waiter to receive
    // a certificate's batches (block).
    tx_get_block_commands: metered_channel::Sender<BlockCommand>,
    // When asking for a certificate's payload we want to retry until we succeed, unless
    // some irrecoverable error occurs. For that reason a backoff policy is defined
    get_block_retry_policy: ExponentialBackoff,
    /// The metrics handler
    metrics: Arc<ExecutorMetrics>,
}

impl Subscriber {
    /// Spawn a new subscriber in a new tokio task.
    #[must_use]
    pub fn spawn(
        store: Store<(CertificateDigest, BatchDigest), Batch>,
        tx_get_block_commands: metered_channel::Sender<BlockCommand>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_consensus: metered_channel::Receiver<ConsensusOutput>,
        tx_executor: metered_channel::Sender<ConsensusOutput>,
        metrics: Arc<ExecutorMetrics>,
        restored_consensus_output: Vec<ConsensusOutput>,
    ) -> JoinHandle<()> {
        let get_block_retry_policy = ExponentialBackoff {
            initial_interval: Duration::from_millis(500),
            randomization_factor: backoff::default::RANDOMIZATION_FACTOR,
            multiplier: backoff::default::MULTIPLIER,
            max_interval: Duration::from_secs(10), // Maximum backoff is 10 seconds
            max_elapsed_time: None, // Never end retrying unless a non recoverable error occurs.
            ..Default::default()
        };

        tokio::spawn(async move {
            Self {
                store,
                rx_reconfigure,
                rx_consensus,
                tx_executor,
                tx_get_block_commands,
                get_block_retry_policy,
                metrics,
            }
            .run(restored_consensus_output)
            .await
            .expect("Failed to run subscriber")
        })
    }

    /// Main loop connecting to the consensus to listen to sequence messages.
    async fn run(
        &mut self,
        restored_consensus_output: Vec<ConsensusOutput>,
    ) -> SubscriberResult<()> {
        // It's important to process the consensus output in strictly ordered
        // fashion to guarantee that we will deliver to the executor the certificates
        // in the same order we received from rx_consensus.

        // First handle any consensus output messages that were restored due to a restart.
        // This needs to happen before we start listening on rx_consensus and receive messages
        // sequenced after these.
        if let Err(err) = self
            .recover_from_consensus_output(restored_consensus_output)
            .await
        {
            error!("Executor subscriber is shutting down: {err}");
            return Ok(());
        }

        loop {
            tokio::select! {
                // Receive the ordered sequence of consensus messages from a consensus node.
                Some(message) = self.rx_consensus.recv() => {
                    if let Err(err) = self.download_payload_and_forward(message).await {
                        error!("Executor subscriber is shutting down: {err}");
                        return Ok(());
                    }
                },

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    if let ReconfigureNotification::Shutdown = message {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Reads all the restored_consensus_output one by one, fetches their payload
    /// in order, and delivers them to the tx_executor channel. This is a sequential
    /// blocking operation. We should expect to block if executor is saturated, but
    /// this is desired to avoid overloading our system making this easier to trace.
    #[instrument(level="info", skip_all, fields(num_of_certificates = restored_consensus_output.len()), err)]
    async fn recover_from_consensus_output(
        &self,
        restored_consensus_output: Vec<ConsensusOutput>,
    ) -> SubscriberResult<()> {
        for message in restored_consensus_output {
            // we are making this a sequential/blocking operation as the number of payloads
            // that needs to be fetched might exceed the size of the waiting list and then
            // we'll never be able to empty it until as we'll never reach the following loop.
            // Also throttling the recovery is another measure to ensure we don't flood our
            // network with messages.
            self.download_payload_and_forward(message).await?;

            self.metrics.subscriber_recovered_certificates_count.inc();
        }

        Ok(())
    }

    /// Downloads the payload from the worker and forwards the output to the
    /// executor channel if the operation is successful. An error is returned
    /// if we can't forward the output to the executor. If an irrecoverable error
    /// has occurred while downloading the payload then this method panics.
    #[instrument(level="debug", skip_all, fields(certificate_id = ?message.certificate.digest()), err)]
    async fn download_payload_and_forward(&self, message: ConsensusOutput) -> SubscriberResult<()> {
        // Fetch the certificate's payload from the workers. This is done via the
        // block_waiter component. If the batches are not available in the workers then
        // block_waiter will do its best to sync from the other peers. Once all batches
        // are available, we forward the certificate to the Executor Core.
        let result = Self::wait_on_payload(
            self.metrics.clone(),
            self.get_block_retry_policy.clone(),
            self.store.clone(),
            self.tx_get_block_commands.clone(),
            message,
        )
        .await;

        match result {
            Ok(output) => {
                if self.tx_executor.send(output).await.is_err() {
                    return Err(SubscriberError::ClosedChannel(
                        stringify!(self.tx_executor).to_owned(),
                    ));
                }
            }
            Err(err) => {
                panic!("Irrecoverable error occurred while retrieving block payload: {err}");
            }
        }

        Ok(())
    }

    /// The wait_on_payload will try to retrieve the certificate's payload
    /// from the workers via the block_waiter component and relase the
    /// `deliver` once successfully done. Since we want the output to be
    /// sequenced we will not quit this method until we have successfully
    /// fetched the payload.
    async fn wait_on_payload(
        metrics: Arc<ExecutorMetrics>,
        back_off_policy: ExponentialBackoff,
        store: Store<(CertificateDigest, BatchDigest), Batch>,
        tx_get_block_commands: metered_channel::Sender<BlockCommand>,
        deliver: ConsensusOutput,
    ) -> SubscriberResult<ConsensusOutput> {
        // the latency will be measured automatically once the guard
        // goes out of scope and dropped
        let _start_guard = metrics.subscriber_download_payload_latency.start_timer();
        let mut attempts_count = 0;

        let get_block = move || {
            let message = deliver.clone();
            let certificate_id = message.certificate.digest();
            let tx_get_block = tx_get_block_commands.clone();
            let batch_store = store.clone();
            let executor_metrics = metrics.clone();
            let attempts = {
                attempts_count += 1;
                attempts_count
            };

            let span = debug_span!("get_block", attempt = attempts);
            async move {
                let (sender, receiver) = oneshot::channel();

                tx_get_block
                    .send(BlockCommand::GetBlock {
                        id: certificate_id,
                        sender,
                    })
                    .await
                    .map_err(|err| {
                        Error::permanent(PayloadRetrieveError(certificate_id, err.to_string()))
                    })?;

                match receiver.await.map_err(|err| {
                    Error::permanent(PayloadRetrieveError(certificate_id, err.to_string()))
                })? {
                    Ok(block) => {
                        // we successfully received the payload. Now let's add to store
                        batch_store
                            .write_all(
                                block
                                    .batches
                                    .into_iter()
                                    .map(|b| ((certificate_id, b.id), b.transactions)),
                            )
                            .await
                            .map_err(|err| Error::permanent(SubscriberError::from(err)))?;

                        executor_metrics
                            .subscriber_download_payload_attempts
                            .observe(attempts as f64);

                        Ok(message)
                    }
                    Err(err) => {
                        // whatever the error might be at this point we don't
                        // have many options apart from retrying.
                        error!("Error while retrieving block via block waiter: {}", err);
                        Err(Error::transient(PayloadRetrieveError(
                            certificate_id,
                            err.to_string(),
                        )))
                    }
                }
            }
            .instrument(span)
        };

        backoff::future::retry(back_off_policy, get_block).await
    }
}
