// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_waiter::{BatchMessage, BatchMessageError, BatchResult, BlockWaiter, Transaction},
    certificate_waiter::CertificateWaiter,
    core::Core,
    error::DagError,
    garbage_collector::GarbageCollector,
    header_waiter::HeaderWaiter,
    helper::Helper,
    messages::{Certificate, Header, Vote},
    payload_receiver::PayloadReceiver,
    proposer::Proposer,
    synchronizer::Synchronizer,
};
use async_trait::async_trait;
use bytes::Bytes;
use config::{Committee, Parameters, WorkerId};
use crypto::{
    traits::{EncodeDecodeBase64, Signer, VerifyingKey},
    Digest, SignatureService,
};
use futures::sink::SinkExt as _;
use network::{MessageHandler, Receiver as NetworkReceiver, Writer};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr},
    sync::{atomic::AtomicU64, Arc},
};
use store::Store;
use thiserror::Error;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::info;

/// The default channel capacity for each channel of the primary.
pub const CHANNEL_CAPACITY: usize = 1_000;

/// The round number.
pub type Round = u64;

#[derive(Debug, Serialize, Deserialize)]
#[serde(bound(deserialize = "PublicKey: VerifyingKey"))]
pub enum PrimaryMessage<PublicKey: VerifyingKey> {
    Header(Header<PublicKey>),
    Vote(Vote<PublicKey>),
    Certificate(Certificate<PublicKey>),
    CertificatesRequest(Vec<Digest>, /* requestor */ PublicKey),
}

/// The messages sent by the primary to its workers.
#[derive(Debug, Serialize, Deserialize)]
pub enum PrimaryWorkerMessage<PublicKey> {
    /// The primary indicates that the worker need to sync the target missing batches.
    Synchronize(Vec<Digest>, /* target */ PublicKey),
    /// The primary indicates a round update.
    Cleanup(Round),
    /// The primary requests a batch from the worker
    RequestBatch(Digest),
    /// Delete the batches, dictated from the provided vector of digest, from the worker node
    DeleteBatches(Vec<Digest>),
}

/// The messages sent by the workers to their primary.
#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerPrimaryMessage {
    /// The worker indicates it sealed a new batch.
    OurBatch(Digest, WorkerId),
    /// The worker indicates it received a batch's digest from another authority.
    OthersBatch(Digest, WorkerId),
    /// The worker sends a requested batch
    RequestedBatch(Digest, Vec<Transaction>),
    /// When batches are successfully deleted, this message is sent dictating the
    /// batches that have been deleted from the worker.
    DeletedBatches(Vec<Digest>),
    /// An error has been returned by worker
    Error(WorkerPrimaryError),
}

#[derive(Debug, Serialize, Deserialize, Error, Clone, PartialEq)]
pub enum WorkerPrimaryError {
    #[error("Batch with id {0} has not been found")]
    RequestedBatchNotFound(Digest),

    #[error("An error occurred while deleting batches. None deleted")]
    ErrorWhileDeletingBatches(Vec<Digest>),
}

// A type alias marking the "payload" tokens sent by workers to their primary as batch acknowledgements
pub type PayloadToken = u8;

pub struct Primary;

impl Primary {
    const INADDR_ANY: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));

    pub fn spawn<PublicKey: VerifyingKey, Signatory: Signer<PublicKey::Sig> + Send + 'static>(
        name: PublicKey,
        signer: Signatory,
        committee: Committee<PublicKey>,
        parameters: Parameters,
        header_store: Store<Digest, Header<PublicKey>>,
        certificate_store: Store<Digest, Certificate<PublicKey>>,
        payload_store: Store<(Digest, WorkerId), PayloadToken>,
        tx_consensus: Sender<Certificate<PublicKey>>,
        rx_consensus: Receiver<Certificate<PublicKey>>,
    ) {
        let (tx_others_digests, rx_others_digests) = channel(CHANNEL_CAPACITY);
        let (tx_our_digests, rx_our_digests) = channel(CHANNEL_CAPACITY);
        let (tx_parents, rx_parents) = channel(CHANNEL_CAPACITY);
        let (tx_headers, rx_headers) = channel(CHANNEL_CAPACITY);
        let (tx_sync_headers, rx_sync_headers) = channel(CHANNEL_CAPACITY);
        let (tx_sync_certificates, rx_sync_certificates) = channel(CHANNEL_CAPACITY);
        let (tx_headers_loopback, rx_headers_loopback) = channel(CHANNEL_CAPACITY);
        let (tx_certificates_loopback, rx_certificates_loopback) = channel(CHANNEL_CAPACITY);
        let (tx_primary_messages, rx_primary_messages) = channel(CHANNEL_CAPACITY);
        let (tx_cert_requests, rx_cert_requests) = channel(CHANNEL_CAPACITY);
        let (_tx_batch_commands, rx_batch_commands) = channel(CHANNEL_CAPACITY);
        let (tx_batches, rx_batches) = channel(CHANNEL_CAPACITY);

        // Write the parameters to the logs.
        parameters.tracing();

        // Atomic variable use to synchronize all tasks with the latest consensus round. This is only
        // used for cleanup. The only task that write into this variable is `GarbageCollector`.
        let consensus_round = Arc::new(AtomicU64::new(0));

        // Spawn the network receiver listening to messages from the other primaries.
        let mut address = committee
            .primary(&name)
            .expect("Our public key or worker id is not in the committee")
            .primary_to_primary;
        address.set_ip(Primary::INADDR_ANY);
        NetworkReceiver::spawn(
            address,
            /* handler */
            PrimaryReceiverHandler {
                tx_primary_messages,
                tx_cert_requests,
            },
        );
        info!(
            "Primary {} listening to primary messages on {}",
            name.encode_base64(),
            address
        );

        // Spawn the network receiver listening to messages from our workers.
        let mut address = committee
            .primary(&name)
            .expect("Our public key or worker id is not in the committee")
            .worker_to_primary;
        address.set_ip(Primary::INADDR_ANY);
        NetworkReceiver::spawn(
            address,
            /* handler */
            WorkerReceiverHandler {
                tx_our_digests,
                tx_others_digests,
                tx_batches,
            },
        );
        info!(
            "Primary {} listening to workers messages on {}",
            name.encode_base64(),
            address
        );

        // The `Synchronizer` provides auxiliary methods helping the `Core` to sync.
        let synchronizer = Synchronizer::new(
            name.clone(),
            &committee,
            certificate_store.clone(),
            payload_store.clone(),
            /* tx_header_waiter */ tx_sync_headers,
            /* tx_certificate_waiter */ tx_sync_certificates,
        );

        // The `SignatureService` is used to require signatures on specific digests.
        let signature_service = SignatureService::new(signer);

        // The `Core` receives and handles headers, votes, and certificates from the other primaries.
        Core::spawn(
            name.clone(),
            committee.clone(),
            header_store.clone(),
            certificate_store.clone(),
            synchronizer,
            signature_service.clone(),
            consensus_round.clone(),
            parameters.gc_depth,
            /* rx_primaries */ rx_primary_messages,
            /* rx_header_waiter */ rx_headers_loopback,
            /* rx_certificate_waiter */ rx_certificates_loopback,
            /* rx_proposer */ rx_headers,
            tx_consensus,
            /* tx_proposer */ tx_parents,
        );

        // Keeps track of the latest consensus round and allows other tasks to clean up their their internal state
        GarbageCollector::spawn(&name, &committee, consensus_round.clone(), rx_consensus);

        // Receives batch digests from other workers. They are only used to validate headers.
        PayloadReceiver::spawn(
            payload_store.clone(),
            /* rx_workers */ rx_others_digests,
        );

        // Retrieves a block's data by contacting the worker nodes that contain the
        // underlying batches and their transactions.
        BlockWaiter::spawn(
            name.clone(),
            committee.clone(),
            certificate_store.clone(),
            rx_batch_commands,
            rx_batches,
        );

        // Whenever the `Synchronizer` does not manage to validate a header due to missing parent certificates of
        // batch digests, it commands the `HeaderWaiter` to synchronize with other nodes, wait for their reply, and
        // re-schedule execution of the header once we have all missing data.
        HeaderWaiter::spawn(
            name.clone(),
            committee.clone(),
            header_store,
            payload_store,
            consensus_round.clone(),
            parameters.gc_depth,
            parameters.sync_retry_delay,
            parameters.sync_retry_nodes,
            /* rx_synchronizer */ rx_sync_headers,
            /* tx_core */ tx_headers_loopback,
        );

        // The `CertificateWaiter` waits to receive all the ancestors of a certificate before looping it back to the
        // `Core` for further processing.
        CertificateWaiter::spawn(
            certificate_store.clone(),
            consensus_round,
            parameters.gc_depth,
            /* rx_synchronizer */ rx_sync_certificates,
            /* tx_core */ tx_certificates_loopback,
        );

        // When the `Core` collects enough parent certificates, the `Proposer` generates a new header with new batch
        // digests from our workers and it back to the `Core`.
        Proposer::spawn(
            name.clone(),
            &committee,
            signature_service,
            parameters.header_size,
            parameters.max_header_delay,
            /* rx_core */ rx_parents,
            /* rx_workers */ rx_our_digests,
            /* tx_core */ tx_headers,
        );

        // The `Helper` is dedicated to reply to certificates requests from other primaries.
        Helper::spawn(committee.clone(), certificate_store, rx_cert_requests);

        // NOTE: This log entry is used to compute performance.
        info!(
            "Primary {} successfully booted on {}",
            name.encode_base64(),
            committee
                .primary(&name)
                .expect("Our public key or worker id is not in the committee")
                .primary_to_primary
                .ip()
        );
    }
}

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
struct PrimaryReceiverHandler<PublicKey: VerifyingKey> {
    tx_primary_messages: Sender<PrimaryMessage<PublicKey>>,
    tx_cert_requests: Sender<(Vec<Digest>, PublicKey)>,
}

#[async_trait]
impl<PublicKey: VerifyingKey> MessageHandler for PrimaryReceiverHandler<PublicKey> {
    async fn dispatch(&self, writer: &mut Writer, serialized: Bytes) -> Result<(), Box<dyn Error>> {
        // Reply with an ACK.
        let _ = writer.send(Bytes::from("Ack")).await;

        // Deserialize and parse the message.
        match bincode::deserialize(&serialized).map_err(DagError::SerializationError)? {
            PrimaryMessage::CertificatesRequest(missing, requestor) => self
                .tx_cert_requests
                .send((missing, requestor))
                .await
                .expect("Failed to send primary message"),
            request => self
                .tx_primary_messages
                .send(request)
                .await
                .expect("Failed to send certificate"),
        }
        Ok(())
    }
}

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
struct WorkerReceiverHandler {
    tx_our_digests: Sender<(Digest, WorkerId)>,
    tx_others_digests: Sender<(Digest, WorkerId)>,
    tx_batches: Sender<BatchResult>,
}

#[async_trait]
impl MessageHandler for WorkerReceiverHandler {
    async fn dispatch(
        &self,
        _writer: &mut Writer,
        serialized: Bytes,
    ) -> Result<(), Box<dyn Error>> {
        // Deserialize and parse the message.
        match bincode::deserialize(&serialized).map_err(DagError::SerializationError)? {
            WorkerPrimaryMessage::OurBatch(digest, worker_id) => self
                .tx_our_digests
                .send((digest, worker_id))
                .await
                .expect("Failed to send workers' digests"),
            WorkerPrimaryMessage::OthersBatch(digest, worker_id) => self
                .tx_others_digests
                .send((digest, worker_id))
                .await
                .expect("Failed to send workers' digests"),
            WorkerPrimaryMessage::RequestedBatch(digest, transactions) => self
                .tx_batches
                .send(Ok(BatchMessage {
                    id: digest,
                    transactions,
                }))
                .await
                .expect("Failed to send batch result"),
            WorkerPrimaryMessage::DeletedBatches(_) => {
                // TODO: send the deleted batches to the appropriate channel
            }
            WorkerPrimaryMessage::Error(error) => match error.clone() {
                WorkerPrimaryError::RequestedBatchNotFound(digest) => self
                    .tx_batches
                    .send(Err(BatchMessageError { id: digest }))
                    .await
                    .expect("Failed to send batch result"),
                WorkerPrimaryError::ErrorWhileDeletingBatches(_) => {
                    // TODO: send the error to the appropriate channel
                }
            },
        }
        Ok(())
    }
}
