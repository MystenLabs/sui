// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::cmp::min;
use std::cmp::Ordering;
use std::net::SocketAddr;
use std::sync::atomic::Ordering as AtomicOrdering;
use std::sync::Arc;
use std::time::Duration;
use sui_network::transport;
use sui_network::transport::{RwChannel, TcpDataStream};
use sui_types::base_types::SequenceNumber;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{ConfirmationTransaction, ConsensusOutput, ConsensusSync};
use sui_types::serialize::{deserialize_message, serialize_consensus_sync, SerializedMessage};
use sui_types::{fp_bail, fp_ensure};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

#[cfg(test)]
#[path = "unit_tests/consensus_tests.rs"]
pub mod consensus_tests;

/// The possible successful outcome when processing a consensus message.
enum ProcessingOutcome {
    /// All went well (or at least there is nothing to do on our side).
    Ok,
    /// We missed some outputs and need to sync with the consensus node.
    MissingOutputs,
}

/// The `ConsensusClient` receives certificates sequenced by the consensus and updates
/// the authority's database. The client assumes that the messages it receives have
/// already been authenticated (ie. they really come from a trusted consensus node) and
/// integrity-validated (ie. no corrupted messages).
pub struct ConsensusClient {
    /// The (global) authority state to update the locks of shared objects.
    state: Arc<AuthorityState>,
    /// The index of the latest consensus message we processed.
    last_consensus_index: SequenceNumber,
}

impl Drop for ConsensusClient {
    fn drop(&mut self) {
        self.state
            .consensus_guardrail
            .fetch_sub(1, AtomicOrdering::SeqCst);
    }
}

impl ConsensusClient {
    /// Create a new consensus handler with the input authority state.
    pub fn new(state: Arc<AuthorityState>) -> SuiResult<Self> {
        // Ensure there is a single consensus client modifying the state.
        let status = state
            .consensus_guardrail
            .fetch_add(1, AtomicOrdering::SeqCst);
        fp_ensure!(status == 0, SuiError::OnlyOneConsensusClientPermitted);

        // Load the last consensus index from storage.
        let last_consensus_index = state.last_consensus_index()?;

        // Return a consensus client only if all went well (safety-critical).
        Ok(Self {
            state,
            last_consensus_index,
        })
    }

    /// Spawn the consensus client in a new tokio task.
    pub fn spawn(
        mut handler: Self,
        address: SocketAddr,
        buffer_size: usize,
    ) -> JoinHandle<SuiResult<()>> {
        info!("Consensus client connecting to {address}");
        tokio::spawn(async move { handler.run(address, buffer_size).await })
    }

    /// Synchronize with the consensus in case we missed part of its output sequence.
    /// It is safety-critical that we process the consensus' outputs in the complete
    /// and right order.
    async fn synchronize(&mut self, connection: &mut TcpDataStream) -> SuiResult<()> {
        let request = ConsensusSync {
            sequence_number: self.last_consensus_index,
        };
        let bytes = Bytes::from(serialize_consensus_sync(&request));
        connection
            .sink()
            .send(bytes)
            .await
            .map_err(|e| SuiError::ClientIoError {
                error: e.to_string(),
            })
    }

    /// Process a single sequenced certificate.
    async fn handle_consensus_message(&mut self, bytes: Bytes) -> SuiResult<ProcessingOutcome> {
        // We first deserialize the consensus output message. If deserialization fails
        // we may be have a liveness issue. We stop processing of this certificate to
        // ensure safety, and the synchronizer will try again to ask for that certificate.
        let (consensus_message, consensus_index) = match deserialize_message(&*bytes) {
            Ok((_, SerializedMessage::ConsensusOutput(value))) => {
                let ConsensusOutput {
                    message,
                    sequence_number,
                } = *value;
                (message, sequence_number)
            }
            Ok((_, _)) => {
                error!("{}", SuiError::UnexpectedMessage);
                return Err(SuiError::UnexpectedMessage);
            }
            Err(e) => {
                error!("Failed to deserialize consensus output {e}");
                return Err(SuiError::InvalidDecoding);
            }
        };

        // Check that the latest consensus index is as expected; otherwise synchronize.
        match self.last_consensus_index.cmp(&consensus_index) {
            Ordering::Greater => {
                // Something is very wrong. Liveness may be lost (but not safety).
                error!("Consensus index of authority bigger than expected");
                return Ok(ProcessingOutcome::Ok);
            }
            Ordering::Less => {
                debug!("Authority is synchronizing missed sequenced certificates");
                return Ok(ProcessingOutcome::MissingOutputs);
            }
            Ordering::Equal => (),
        }

        // Update the latest consensus index. The authority state will atomically
        // update it in the storage when processing the certificate. It is important to
        // increment the consensus index before deserializing the certificate because
        // the consensus core will increment its own index regardless of deserialization
        // or other protocol-specific failures.
        self.last_consensus_index = self.last_consensus_index.increment();

        // The consensus simply orders bytes, so we first need to deserialize the
        // certificate. If the deserialization fail it is safe to ignore the
        // certificate since all correct authorities will do the same. Remember that a
        // bad authority or client may input random bytes to the consensus.
        let confirmation = match deserialize_message(&*consensus_message) {
            Ok((_, SerializedMessage::Cert(certificate))) => ConfirmationTransaction {
                certificate: *certificate,
            },
            Ok((_, _)) => {
                debug!("{}", SuiError::UnexpectedMessage);
                return Err(SuiError::UnexpectedMessage);
            }
            Err(e) => {
                debug!("Failed to deserialize certificate {e}");
                return Err(SuiError::InvalidDecoding);
            }
        };

        // Process the certificate to set the locks on the shared objects. It also
        // atomically update the last consensus index in storage. It is safety-critical
        // that only this task calls the function below. Safety is preserved even if an
        // authority crashes before this point but after having processed a number of
        // badly serialized certificates, but the synchronizer will have to do more work.
        let certificate = confirmation.certificate;
        self.state
            .handle_consensus_certificate(certificate, self.last_consensus_index)
            .await?;
        Ok(ProcessingOutcome::Ok)
    }

    /// Main loop connecting to the consensus. This mainly acts as a light client.
    async fn run(&mut self, address: SocketAddr, buffer_size: usize) -> SuiResult<()> {
        // TODO: We may also move this logic to `sui-network::transport` to expose a 'stream client'
        // or something like that.

        // The connection waiter ensures we do not attempt to reconnect immediately after failure.
        let mut connection_waiter = ConnectionWaiter::default();

        // Continuously connects to the consensus node.
        'main: loop {
            // Wait a bit before re-attempting connections.
            connection_waiter.wait().await;

            // Subscribe to the consensus' output.
            let mut connection = match transport::connect(address.to_string(), buffer_size).await {
                Ok(connection) => connection,
                Err(e) => {
                    warn!(
                        "Failed to subscribe to consensus output (retry {}): {}",
                        connection_waiter.status(),
                        e
                    );
                    continue 'main;
                }
            };

            // Listen to sequenced certificates and process them.
            loop {
                let bytes = match connection.stream().next().await {
                    Some(Ok(data)) => Bytes::from(data),
                    Some(Err(e)) => {
                        warn!("Failed to receive data from consensus: {e}");
                        continue 'main;
                    }
                    None => {
                        debug!("Connection dropped by consensus");
                        continue 'main;
                    }
                };

                match self.handle_consensus_message(bytes).await {
                    // Log the errors that are our faults (not the client's).
                    Err(SuiError::StorageError(e)) => {
                        error!("{e}");

                        // If we have a store error we cannot continue processing other
                        // outputs from consensus. We may otherwise attribute locks to
                        // shared objects that are different from other authorities. It
                        // is however safe to ask for that certificate again and re-process
                        // it (the core is idempotent).
                        fp_bail!(SuiError::StorageError(e));
                    }
                    // Log the errors that are the client's fault (not ours). This is
                    // only for debug purposes: all correct authorities will do the same.
                    Err(e) => debug!("{e}"),
                    // The authority missed some consensus outputs and needs to sync.
                    Ok(ProcessingOutcome::MissingOutputs) => {
                        if let Err(e) = self.synchronize(&mut connection).await {
                            warn!("Failed to send sync request to consensus: {e}");
                            continue 'main;
                        }
                        connection_waiter.reset();
                    }
                    // Nothing to do.
                    Ok(ProcessingOutcome::Ok) => connection_waiter.reset(),
                }
            }
        }
    }
}

/// Make the network client wait a bit before re-attempting network connections.
pub struct ConnectionWaiter {
    /// The minimum delay to wait before re-attempting a connection.
    min_delay: u64,
    /// The maximum delay to wait before re-attempting a connection.
    max_delay: u64,
    /// The actual delay we wait before re-attempting a connection.
    delay: u64,
    /// The number of times we attempted to make a connection.
    retry: usize,
}

impl Default for ConnectionWaiter {
    fn default() -> Self {
        Self::new(/* min_delay */ 200, /* max_delay */ 60_000)
    }
}

impl ConnectionWaiter {
    /// Create a new connection waiter.
    pub fn new(min_delay: u64, max_delay: u64) -> Self {
        Self {
            min_delay,
            max_delay,
            delay: 0,
            retry: 0,
        }
    }

    /// Return the number of failed attempts.
    pub fn status(&self) -> &usize {
        &self.retry
    }

    /// Wait for a bit (depending on the number of failed connections).
    pub async fn wait(&mut self) {
        if self.delay != 0 {
            sleep(Duration::from_millis(self.delay)).await;
        }

        self.delay = match self.delay {
            0 => self.min_delay,
            _ => min(2 * self.delay, self.max_delay),
        };
        self.retry += 1;
    }

    /// Reset the waiter to its initial parameters.
    pub fn reset(&mut self) {
        self.delay = 0;
        self.retry = 0;
    }
}
