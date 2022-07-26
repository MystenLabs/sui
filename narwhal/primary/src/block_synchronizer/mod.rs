// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{
        peers::Peers,
        responses::{CertificatesResponse, PayloadAvailabilityResponse, RequestID},
        PendingIdentifier::{Header, Payload},
    },
    primary::PrimaryMessage,
    utils, PayloadToken, CHANNEL_CAPACITY,
};
use config::{BlockSynchronizerParameters, Committee, WorkerId};
use crypto::{traits::VerifyingKey, Hash};
use futures::{
    future::{join_all, BoxFuture},
    stream::FuturesUnordered,
    FutureExt, StreamExt,
};
use network::{PrimaryNetwork, PrimaryToWorkerNetwork};
use rand::{rngs::SmallRng, SeedableRng};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use store::Store;
use thiserror::Error;
use tokio::{
    sync::{
        mpsc::{channel, Receiver, Sender},
        watch,
    },
    task::JoinHandle,
    time::{sleep, timeout},
};
use tracing::{debug, error, instrument, trace, warn};
use types::{
    BatchDigest, Certificate, CertificateDigest, PrimaryWorkerMessage, ReconfigureNotification,
};

#[cfg(test)]
#[path = "tests/block_synchronizer_tests.rs"]
mod block_synchronizer_tests;
pub mod handler;
pub mod mock;
mod peers;
pub mod responses;

/// The minimum percentage
/// (number of responses received from primary nodes / number of requests sent to primary nodes)
/// that should be reached when requesting the certificates from peers in order to
/// proceed to next state.
const CERTIFICATE_RESPONSES_RATIO_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone)]
pub struct BlockHeader<PublicKey: VerifyingKey> {
    pub certificate: Certificate<PublicKey>,
    /// It designates whether the requested quantity (either the certificate
    /// or the payload) has been retrieved via the local storage. If true,
    /// the it used the storage. If false, then it has been fetched via
    /// the peers.
    pub fetched_from_storage: bool,
}

type ResultSender<T> = Sender<BlockSynchronizeResult<BlockHeader<T>>>;
pub type BlockSynchronizeResult<T> = Result<T, SyncError>;

#[derive(Debug)]
pub enum Command<PublicKey: VerifyingKey> {
    #[allow(dead_code)]
    /// A request to synchronize and output the block headers
    /// This will not perform any attempt to fetch the header's
    /// batches. This component does NOT check whether the
    /// requested block_ids are already synchronized. This is the
    /// consumer's responsibility.
    SynchronizeBlockHeaders {
        block_ids: Vec<CertificateDigest>,
        respond_to: ResultSender<PublicKey>,
    },
    /// A request to synchronize the payload (batches) of the
    /// provided certificates. The certificates are needed in
    /// order to know which batches to ask from the peers
    /// to sync and from which workers.
    /// TODO: We expect to change how batches are stored and
    /// represended (see https://github.com/MystenLabs/narwhal/issues/54
    /// and https://github.com/MystenLabs/narwhal/issues/150 )
    /// and this might relax the requirement to need certificates here.
    ///
    /// This component does NOT check whether the
    //  requested block_ids are already synchronized. This is the
    //  consumer's responsibility.
    #[allow(dead_code)]
    SynchronizeBlockPayload {
        certificates: Vec<Certificate<PublicKey>>,
        respond_to: ResultSender<PublicKey>,
    },
}

// Those states are used for internal purposes only for the component.
// We are implementing a very very naive state machine and go get from
// one state to the other those commands are being used.
enum State<PublicKey: VerifyingKey> {
    HeadersSynchronized {
        request_id: RequestID,
        certificates: HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader<PublicKey>>>,
    },
    PayloadAvailabilityReceived {
        request_id: RequestID,
        certificates: HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader<PublicKey>>>,
        peers: Peers<PublicKey, Certificate<PublicKey>>,
    },
    PayloadSynchronized {
        request_id: RequestID,
        result: BlockSynchronizeResult<BlockHeader<PublicKey>>,
    },
}

#[derive(Debug, Error, Copy, Clone)]
pub enum SyncError {
    #[error("Block with id {block_id} was not returned in any peer response")]
    NoResponse { block_id: CertificateDigest },

    #[error("Block with id {block_id} could not be retrieved, timeout while retrieving result")]
    Timeout { block_id: CertificateDigest },
}

impl SyncError {
    #[allow(dead_code)]
    pub fn block_id(&self) -> CertificateDigest {
        match *self {
            SyncError::NoResponse { block_id } | SyncError::Timeout { block_id } => block_id,
        }
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
enum PendingIdentifier {
    Header(CertificateDigest),
    Payload(CertificateDigest),
}

impl PendingIdentifier {
    #[allow(dead_code)]
    fn id(&self) -> CertificateDigest {
        match self {
            PendingIdentifier::Header(id) | PendingIdentifier::Payload(id) => *id,
        }
    }
}

pub struct BlockSynchronizer<PublicKey: VerifyingKey> {
    /// The public key of this primary.
    name: PublicKey,

    /// The committee information.
    committee: Committee<PublicKey>,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,

    /// Receive the commands for the synchronizer
    rx_commands: Receiver<Command<PublicKey>>,

    /// Receive the requested list of certificates through this channel
    rx_certificate_responses: Receiver<CertificatesResponse<PublicKey>>,

    /// Receive the availability for the requested certificates through this
    /// channel
    rx_payload_availability_responses: Receiver<PayloadAvailabilityResponse<PublicKey>>,

    /// Pending block requests either for header or payload type
    pending_requests: HashMap<PendingIdentifier, Vec<ResultSender<PublicKey>>>,

    /// Requests managers
    map_certificate_responses_senders: HashMap<RequestID, Sender<CertificatesResponse<PublicKey>>>,

    /// Holds the senders to match a batch_availability responses
    map_payload_availability_responses_senders:
        HashMap<RequestID, Sender<PayloadAvailabilityResponse<PublicKey>>>,

    /// Send network requests
    primary_network: PrimaryNetwork,
    worker_network: PrimaryToWorkerNetwork,

    /// The store that holds the certificates
    certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,

    /// The persistent storage for payload markers from workers
    payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,

    /// Timeout when synchronizing the certificates
    certificates_synchronize_timeout: Duration,

    /// Timeout when synchronizing the payload
    payload_synchronize_timeout: Duration,

    /// Timeout when has requested the payload and waiting to receive
    payload_availability_timeout: Duration,
}

impl<PublicKey: VerifyingKey> BlockSynchronizer<PublicKey> {
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_commands: Receiver<Command<PublicKey>>,
        rx_certificate_responses: Receiver<CertificatesResponse<PublicKey>>,
        rx_payload_availability_responses: Receiver<PayloadAvailabilityResponse<PublicKey>>,
        network: PrimaryNetwork,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        parameters: BlockSynchronizerParameters,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                rx_reconfigure,
                rx_commands,
                rx_certificate_responses,
                rx_payload_availability_responses,
                pending_requests: HashMap::new(),
                map_certificate_responses_senders: HashMap::new(),
                map_payload_availability_responses_senders: HashMap::new(),
                primary_network: network,
                worker_network: PrimaryToWorkerNetwork::default(),
                payload_store,
                certificate_store,
                certificates_synchronize_timeout: parameters.certificates_synchronize_timeout,
                payload_synchronize_timeout: parameters.payload_availability_timeout,
                payload_availability_timeout: parameters.payload_availability_timeout,
            }
            .run()
            .await;
        })
    }

    pub async fn run(&mut self) {
        // Waiting is holding futures which are an outcome of processing
        // of other methods, which we want to asynchronously handle them.
        // We expect every future included in this list to produce an
        // outcome of the "next state" to be executed - see the State enum.
        // That allows us to implement a naive state machine mechanism and
        // pass freely arbitrary data across those states for further
        // processing.
        let mut waiting = FuturesUnordered::new();

        loop {
            tokio::select! {
                Some(command) = self.rx_commands.recv() => {
                    match command {
                        Command::SynchronizeBlockHeaders { block_ids, respond_to } => {
                            let fut = self.handle_synchronize_block_headers_command(block_ids, respond_to).await;
                            if fut.is_some() {
                                waiting.push(fut.unwrap());
                            }
                        },
                        Command::SynchronizeBlockPayload { certificates, respond_to } => {
                            let fut = self.handle_synchronize_block_payload_command(certificates, respond_to).await;
                            if fut.is_some() {
                                waiting.push(fut.unwrap());
                            }
                        }
                    }
                },
                Some(response) = self.rx_certificate_responses.recv() => {
                    self.handle_certificates_response(response).await;
                },
                Some(response) = self.rx_payload_availability_responses.recv() => {
                    self.handle_payload_availability_response(response).await;
                },
                Some(state) = waiting.next() => {
                    match state {
                        State::HeadersSynchronized { request_id, certificates } => {
                            debug!("Result for the block headers synchronize request id {request_id}");

                            for (id, result) in certificates {
                                self.notify_requestors_for_result(Header(id), result).await;
                            }
                        },
                        State::PayloadAvailabilityReceived { request_id, certificates, peers } => {
                             debug!("Result for the block payload synchronize request id {request_id}");

                            // now try to synchronise the payload only for the ones that have been found
                            let futures = self.handle_synchronize_block_payloads(request_id, peers).await;
                            for fut in futures {
                                waiting.push(fut);
                            }

                            // notify immediately for block_ids that have been errored or timedout
                            for (id, result) in certificates {
                                if result.is_err() {
                                    self.notify_requestors_for_result(Payload(id), result).await;
                                }
                            }
                        },
                        State::PayloadSynchronized { request_id, result } => {
                            let id = result.as_ref().map_or_else(|e| e.block_id(), |r| r.certificate.digest());

                            debug!("Block payload synchronize result received for certificate id {id} for request id {request_id}");

                            self.notify_requestors_for_result(Payload(id), result).await;
                        },
                    }
                }

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            self.committee = new_committee;
                            tracing::debug!("Committee updated to {}", self.committee);
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                }
            }
        }
    }

    async fn notify_requestors_for_result(
        &mut self,
        request: PendingIdentifier,
        result: BlockSynchronizeResult<BlockHeader<PublicKey>>,
    ) {
        // remove the senders & broadcast result
        if let Some(respond_to) = self.pending_requests.remove(&request) {
            let futures: Vec<_> = respond_to.iter().map(|s| s.send(result.clone())).collect();

            for r in join_all(futures).await {
                if r.is_err() {
                    error!("Couldn't send message to channel [{:?}]", r.err().unwrap());
                }
            }
        }
    }

    // Helper method to mark a request as pending. It returns true if it is the
    // first request for this identifier, otherwise false is returned instead.
    fn resolve_pending_request(
        &mut self,
        identifier: PendingIdentifier,
        respond_to: ResultSender<PublicKey>,
    ) -> bool {
        // add our self anyways to a pending request, as we don't expect to
        // fail down the line of this method (unless crash)
        let e = self.pending_requests.entry(identifier).or_default();
        e.push(respond_to);

        e.len() == 1
    }

    /// This method handles the command to synchronize the payload of the
    /// provided certificates. It finds for which certificates we don't
    /// have an already pending request and broadcasts a message to all
    /// the primary peer nodes to scout which ones have and are able to send
    /// us the payload. It returns a future which is responsible to run the
    /// logic of waiting and gathering the replies from the primary nodes
    /// for the payload availability. This future is returning the next State
    /// to be executed.
    #[instrument(level="debug", skip_all, fields(num_certificates = certificates.len()))]
    async fn handle_synchronize_block_payload_command<'a>(
        &mut self,
        certificates: Vec<Certificate<PublicKey>>,
        respond_to: ResultSender<PublicKey>,
    ) -> Option<BoxFuture<'a, State<PublicKey>>> {
        let mut certificates_to_sync = Vec::new();
        let mut block_ids_to_sync = Vec::new();

        let missing_certificates = self
            .reply_with_payload_already_in_storage(certificates.clone(), respond_to.clone())
            .await;

        for certificate in missing_certificates {
            let block_id = certificate.digest();

            if self.resolve_pending_request(Payload(block_id), respond_to.clone()) {
                certificates_to_sync.push(certificate);
                block_ids_to_sync.push(block_id);
            } else {
                trace!("Nothing to request here, it's already in pending state");
            }
        }

        // nothing new to sync! just return
        if certificates_to_sync.is_empty() {
            trace!("No certificates to sync, will now exit");
            return None;
        } else {
            trace!("Certificate payloads need sync");
        }

        let key = RequestID::from_iter(certificates_to_sync.iter());

        let message = PrimaryMessage::<PublicKey>::PayloadAvailabilityRequest {
            certificate_ids: block_ids_to_sync,
            requestor: self.name.clone(),
        };

        let (sender, receiver) = channel(CHANNEL_CAPACITY);
        // record the request key to forward the results to the dedicated sender
        self.map_payload_availability_responses_senders
            .insert(key, sender);

        // broadcast the message to fetch  the certificates
        let primaries = self.broadcast_batch_request(message).await;

        // now create the future that will wait to gather the responses
        Some(
            Self::wait_for_payload_availability_responses(
                self.payload_availability_timeout,
                key,
                certificates_to_sync,
                primaries,
                receiver,
            )
            .boxed(),
        )
    }

    /// This method handles the command to synchronize the headers
    /// (certificates) for the provided ids. It is deduping the ids for which
    /// it already has a pending request and for the rest is broadcasting a
    /// message to the other peer nodes to fetch the request certificates, if
    /// available. We expect each peer node to respond with the actual
    /// certificates that has available. Also, the method is querying in the
    /// internal storage whether there are any certificates already stored and
    /// available. For the ones found in storage the replies are send directly
    /// back to the consumer. The method returns a future that is running the
    /// process of waiting to gather the node responses and emits the result as
    /// the next State to be executed.
    async fn handle_synchronize_block_headers_command<'a>(
        &mut self,
        block_ids: Vec<CertificateDigest>,
        respond_to: ResultSender<PublicKey>,
    ) -> Option<BoxFuture<'a, State<PublicKey>>> {
        let mut to_sync = Vec::new();

        let missing_block_ids = self
            .reply_with_certificates_already_in_storage(block_ids.clone(), respond_to.clone())
            .await;

        // check if there are pending requests on the block_ids.
        // If yes, then ignore.
        for block_id in missing_block_ids {
            if self.resolve_pending_request(Header(block_id), respond_to.clone()) {
                to_sync.push(block_id);
            } else {
                debug!("Nothing to request here, it's already in pending state");
            }
        }

        // nothing new to sync! just return
        if to_sync.is_empty() {
            return None;
        }

        let key = RequestID::from_iter(to_sync.iter());

        let message = PrimaryMessage::<PublicKey>::CertificatesBatchRequest {
            certificate_ids: to_sync.clone(),
            requestor: self.name.clone(),
        };

        // broadcast the message to fetch  the certificates
        let primaries = self.broadcast_batch_request(message).await;

        let (sender, receiver) = channel(primaries.as_slice().len());

        // record the request key to forward the results to the dedicated sender
        self.map_certificate_responses_senders.insert(key, sender);

        // now create the future that will wait to gather the responses
        Some(
            Self::wait_for_certificate_responses(
                self.certificates_synchronize_timeout,
                key,
                self.committee.clone(),
                to_sync,
                primaries,
                receiver,
            )
            .boxed(),
        )
    }

    /// This method queries the local storage to try and find certificates
    /// identified by the provided block ids. For the ones found it sends
    /// back to the provided `respond_to` sender the certificates directly.
    /// A hashset is returned with the non found block_ids.
    async fn reply_with_certificates_already_in_storage(
        &self,
        block_ids: Vec<CertificateDigest>,
        respond_to: Sender<BlockSynchronizeResult<BlockHeader<PublicKey>>>,
    ) -> HashSet<CertificateDigest> {
        // find the certificates that already exist in storage
        match self.certificate_store.read_all(block_ids.clone()).await {
            Ok(certificates) => {
                let (found, missing): (
                    Vec<(CertificateDigest, Option<Certificate<PublicKey>>)>,
                    Vec<(CertificateDigest, Option<Certificate<PublicKey>>)>,
                ) = block_ids
                    .into_iter()
                    .zip(certificates)
                    .partition(|f| f.1.is_some());

                // Reply back directly with the found from storage certificates
                let futures: Vec<_> = found
                    .into_iter()
                    .flat_map(|(_, c)| c)
                    .map(|c| {
                        respond_to.send(Ok(BlockHeader {
                            certificate: c,
                            fetched_from_storage: true,
                        }))
                    })
                    .collect();

                for r in join_all(futures).await {
                    if let Err(err) = r {
                        error!("Couldn't send message to channel [{:?}]", err);
                    }
                }

                // reply back with the missing certificates
                missing.into_iter().map(|e| e.0).collect()
            }
            Err(err) => {
                error!("Couldn't fetch certificates: {err}");

                // report all as missing so we can at least try to fetch from peers.
                HashSet::from_iter(block_ids.iter().cloned())
            }
        }
    }

    /// For each provided certificate, via the certificates vector, it queries
    /// the internal storage to identify whether all the payload batches are
    /// available. For the certificates that their full payload is found, then
    /// a reply is immediately sent to the consumer via the provided respond_to
    /// channel. For the ones that haven't been found, are returned back on the
    /// returned vector.
    #[instrument(level = "debug", skip_all)]
    async fn reply_with_payload_already_in_storage(
        &self,
        certificates: Vec<Certificate<PublicKey>>,
        respond_to: Sender<BlockSynchronizeResult<BlockHeader<PublicKey>>>,
    ) -> Vec<Certificate<PublicKey>> {
        let mut missing_payload_certs = Vec::new();
        let mut futures = Vec::new();

        for certificate in certificates {
            let payload: Vec<(BatchDigest, WorkerId)> =
                certificate.header.payload.clone().into_iter().collect();

            let payload_available = if certificate.header.author == self.name {
                trace!(
                    "Certificate with id {} is our own, no need to check in storage.",
                    certificate.digest()
                );
                true
            } else {
                trace!(
                    "Certificate with id {} not our own, checking in storage.",
                    certificate.digest()
                );
                match self.payload_store.read_all(payload).await {
                    Ok(payload_result) => {
                        payload_result.into_iter().all(|x| x.is_some()).to_owned()
                    }
                    Err(err) => {
                        error!("Error occurred when querying payloads: {err}");
                        false
                    }
                }
            };

            if !payload_available {
                trace!(
                    "Payload not available for certificate with id {}",
                    certificate.digest()
                );
                missing_payload_certs.push(certificate);
            } else {
                trace!("Payload is available on storage for certificate with id {}, now replying back immediately", certificate.digest());
                futures.push(respond_to.send(Ok(BlockHeader {
                    certificate,
                    fetched_from_storage: true,
                })));
            }
        }

        for r in join_all(futures).await {
            if r.is_err() {
                error!("Couldn't send message to channel {:?}", r.err());
            }
        }

        missing_payload_certs
    }

    // Broadcasts a message to all the other primary nodes.
    // It returns back the primary names to which we have sent the requests.
    #[instrument(level = "debug", skip_all)]
    async fn broadcast_batch_request(
        &mut self,
        message: PrimaryMessage<PublicKey>,
    ) -> Vec<PublicKey> {
        // Naively now just broadcast the request to all the primaries

        let (primaries_names, primaries_addresses) = self
            .committee
            .others_primaries(&self.name)
            .into_iter()
            .map(|(name, address)| (name, address.primary_to_primary))
            .unzip();

        self.primary_network
            .unreliable_broadcast(primaries_addresses, &message)
            .await;

        primaries_names
    }

    #[instrument(level="debug", skip_all, fields(request_id = ?request_id))]
    async fn handle_synchronize_block_payloads<'a>(
        &mut self,
        request_id: RequestID,
        mut peers: Peers<PublicKey, Certificate<PublicKey>>,
    ) -> Vec<BoxFuture<'a, State<PublicKey>>> {
        // Important step to do that first, so we give the opportunity
        // to other future requests (with same set of ids) making a request.
        self.map_payload_availability_responses_senders
            .remove(&request_id);

        // Rebalance the CertificateDigests to ensure that
        // those are uniquely distributed across the peers.
        peers.rebalance_values();

        for peer in peers.peers().values() {
            self.send_synchronize_payload_requests(peer.clone().name, peer.assigned_values())
                .await
        }

        peers
            .unique_values()
            .into_iter()
            .map(|certificate| {
                Self::wait_for_block_payload(
                    self.payload_synchronize_timeout,
                    request_id,
                    self.payload_store.clone(),
                    certificate,
                )
                .boxed()
            })
            .collect()
    }

    /// This method sends the necessary requests to the worker nodes to
    /// synchronize the missing batches. The batches will be synchronized
    /// from the dictated primary_peer_name.
    ///
    /// # Arguments
    ///
    /// * `primary_peer_name` - The primary from which we are looking to sync the batches.
    /// * `certificates` - The certificates for which we want to sync their batches.
    #[instrument(level = "debug", skip_all)]
    async fn send_synchronize_payload_requests(
        &mut self,
        primary_peer_name: PublicKey,
        certificates: Vec<Certificate<PublicKey>>,
    ) {
        let batches_by_worker = utils::map_certificate_batches_by_worker(certificates.as_slice());

        for (worker_id, batch_ids) in batches_by_worker {
            let worker_address = self
                .committee
                .worker(&self.name, &worker_id)
                .expect("Worker id not found")
                .primary_to_worker;

            let message =
                PrimaryWorkerMessage::Synchronize(batch_ids.clone(), primary_peer_name.clone());
            self.worker_network.send(worker_address, &message).await;

            debug!(
                "Sent request for batch ids {:?} to worker id {}",
                batch_ids, worker_id
            );
        }
    }

    async fn wait_for_block_payload<'a>(
        payload_synchronize_timeout: Duration,
        request_id: RequestID,
        payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
        certificate: Certificate<PublicKey>,
    ) -> State<PublicKey> {
        let futures = certificate
            .header
            .payload
            .iter()
            .map(|(batch_digest, worker_id)| payload_store.notify_read((*batch_digest, *worker_id)))
            .collect::<Vec<_>>();

        // Wait for all the items to sync - have a timeout
        let result = timeout(payload_synchronize_timeout, join_all(futures)).await;
        if result.is_err()
            || result
                .unwrap()
                .into_iter()
                .any(|r| r.map_or_else(|_| true, |f| f.is_none()))
        {
            return State::PayloadSynchronized {
                request_id,
                result: Err(SyncError::Timeout {
                    block_id: certificate.digest(),
                }),
            };
        }

        State::PayloadSynchronized {
            request_id,
            result: Ok(BlockHeader {
                certificate,
                fetched_from_storage: false,
            }),
        }
    }

    #[instrument(level = "debug", skip_all)]
    async fn handle_payload_availability_response(
        &mut self,
        response: PayloadAvailabilityResponse<PublicKey>,
    ) {
        let sender = self
            .map_payload_availability_responses_senders
            .get(&response.request_id());

        if let Some(s) = sender {
            debug!(
                "Received response for request with id {}: {:?}",
                response.request_id(),
                response.clone()
            );
            if let Err(e) = s.send(response).await {
                error!("Could not send the response to the sender {:?}", e);
            }
        } else {
            warn!("Couldn't find a sender to channel the response. Will drop the message.");
        }
    }

    #[instrument(level = "debug", skip_all)]
    async fn handle_certificates_response(&mut self, response: CertificatesResponse<PublicKey>) {
        let sender = self
            .map_certificate_responses_senders
            .get(&response.request_id());

        if let Some(s) = sender {
            if let Err(e) = s.send(response).await {
                error!("Could not send the response to the sender {:?}", e);
            }
        } else {
            warn!("Couldn't find a sender to channel the response. Will drop the message.");
        }
    }

    async fn wait_for_certificate_responses(
        fetch_certificates_timeout: Duration,
        request_id: RequestID,
        committee: Committee<PublicKey>,
        block_ids: Vec<CertificateDigest>,
        primaries_sent_requests_to: Vec<PublicKey>,
        mut receiver: Receiver<CertificatesResponse<PublicKey>>,
    ) -> State<PublicKey> {
        let total_expected_certificates = block_ids.len();
        let mut num_of_responses: u32 = 0;
        let num_of_requests_sent: u32 = primaries_sent_requests_to.len() as u32;

        let timer = sleep(fetch_certificates_timeout);
        tokio::pin!(timer);

        let mut peers = Peers::<PublicKey, Certificate<PublicKey>>::new(SmallRng::from_entropy());

        loop {
            tokio::select! {
                Some(response) = receiver.recv() => {
                    trace!("Received response: {:?}", &response);

                    if peers.contains_peer(&response.from) {
                        // skip , we already got an answer from this peer
                        continue;
                    }

                    // check whether the peer is amongst the one we are expecting
                    // response from. That shouldn't really happen, since the
                    // responses we get are filtered by the request id, but still
                    // worth double checking
                    if !primaries_sent_requests_to.iter().any(|p|p.eq(&response.from)) {
                        warn!("Not expected reply from this peer, will skip response");
                        continue;
                    }

                    num_of_responses += 1;

                    match response.validate_certificates(&committee) {
                        Ok(certificates) => {
                            // Ensure we got responses for the certificates we asked for.
                            // Even if we have found one certificate that doesn't match
                            // we reject the payload - it shouldn't happen.
                            if certificates.iter().any(|c|!block_ids.contains(&c.digest())) {
                                warn!("Will not process certificates, found at least one that we haven't asked for");
                                continue;
                            }

                            // add them as a new peer
                            peers.add_peer(response.from.clone(), certificates);

                            // We have received all possible responses
                            if (peers.unique_values().len() == total_expected_certificates &&
                            Self::reached_response_ratio(num_of_responses, num_of_requests_sent))
                            || num_of_responses == num_of_requests_sent
                            {
                                let result = Self::resolve_block_synchronize_result(&peers, block_ids, false);

                                return State::HeadersSynchronized {
                                    request_id,
                                    certificates: result,
                                };
                            }
                        },
                        Err(err) => {
                            warn!("Got invalid certificates from peer: {:?}", err);
                        }
                    }
                },
                () = &mut timer => {
                    let result = Self::resolve_block_synchronize_result(&peers, block_ids, true);

                    return State::HeadersSynchronized {
                        request_id,
                        certificates: result,
                    };
                }
            }
        }
    }

    async fn wait_for_payload_availability_responses(
        fetch_certificates_timeout: Duration,
        request_id: RequestID,
        certificates: Vec<Certificate<PublicKey>>,
        primaries_sent_requests_to: Vec<PublicKey>,
        mut receiver: Receiver<PayloadAvailabilityResponse<PublicKey>>,
    ) -> State<PublicKey> {
        let total_expected_block_ids = certificates.len();
        let mut num_of_responses: u32 = 0;
        let num_of_requests_sent: u32 = primaries_sent_requests_to.len() as u32;
        let certificates_by_id: HashMap<CertificateDigest, Certificate<PublicKey>> = certificates
            .iter()
            .map(|c| (c.digest(), c.clone()))
            .collect();
        let block_ids: Vec<CertificateDigest> = certificates_by_id
            .iter()
            .map(|(id, _)| id.to_owned())
            .collect();

        let timer = sleep(fetch_certificates_timeout);
        tokio::pin!(timer);

        let mut peers = Peers::<PublicKey, Certificate<PublicKey>>::new(SmallRng::from_entropy());

        loop {
            tokio::select! {
                Some(response) = receiver.recv() => {
                    if peers.contains_peer(&response.from) {
                        // skip , we already got an answer from this peer
                        continue;
                    }

                    // check whether the peer is amongst the one we are expecting
                    // response from. That shouldn't really happen, since the
                    // responses we get are filtered by the request id, but still
                    // worth double checking
                    if !primaries_sent_requests_to.iter().any(|p|p.eq(&response.from)) {
                        continue;
                    }

                    num_of_responses += 1;

                    // Ensure we got responses for the certificates we asked for.
                    // Even if we have found one certificate that doesn't match
                    // we reject the payload - it shouldn't happen. Also, add the
                    // found ones in a vector.
                    let mut available_certs_for_peer = Vec::new();
                    for id in response.available_block_ids() {
                        if let Some(c) = certificates_by_id.get(&id) {
                            available_certs_for_peer.push(c.clone());
                        } else {
                            // We should expect to have found every
                            // responded id to our list of certificates.
                            continue;
                        }
                    }

                    // add them as a new peer
                    peers.add_peer(response.from.clone(), available_certs_for_peer);

                    // We have received all possible responses
                    if (peers.unique_values().len() == total_expected_block_ids &&
                    Self::reached_response_ratio(num_of_responses, num_of_requests_sent))
                    || num_of_responses == num_of_requests_sent
                    {
                        let result = Self::resolve_block_synchronize_result(&peers, block_ids, false);

                        return State::PayloadAvailabilityReceived {
                            request_id,
                            certificates: result,
                            peers,
                        };
                    }
                },
                () = &mut timer => {
                    let result = Self::resolve_block_synchronize_result(&peers, block_ids, true);

                    return State::PayloadAvailabilityReceived {
                        request_id,
                        certificates: result,
                        peers,
                    };
                }
            }
        }
    }

    // It creates a map which holds for every expected block_id the corresponding
    // result. The actually found certificates are hold inside the provided peers
    // structure where for each peer (primary node) we keep the certificates that
    // is able to serve. We reduce those to unique values (certificates) and
    // produce the map. If a certificate has not been found amongst the ones
    // that the peers can serve, then an error SyncError is produced for it. The
    // error type changes according to the provided `timeout` value. If is true,
    // the it means that the maximum time reached when waiting to get the results
    // from the required peers. In this case the error result will be a Timeout.
    // Otherwise it will be an Error. The outcome map can be used then to
    // communicate the result for each requested certificate (if found, then
    // certificate it self , or if error then the type of the error).
    fn resolve_block_synchronize_result(
        peers: &Peers<PublicKey, Certificate<PublicKey>>,
        block_ids: Vec<CertificateDigest>,
        timeout: bool,
    ) -> HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader<PublicKey>>> {
        let mut certificates_by_id: HashMap<CertificateDigest, Certificate<PublicKey>> = peers
            .unique_values()
            .into_iter()
            .map(|c| (c.digest(), c))
            .collect();

        let mut result: HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader<PublicKey>>> =
            HashMap::new();

        for block_id in block_ids {
            // if not found, then this is an Error - couldn't be retrieved
            // by any peer - suspicious!
            if let Some(certificate) = certificates_by_id.remove(&block_id) {
                result.insert(
                    block_id,
                    Ok(BlockHeader {
                        certificate,
                        fetched_from_storage: false,
                    }),
                );
            } else if timeout {
                result.insert(block_id, Err(SyncError::Timeout { block_id }));
            } else {
                result.insert(block_id, Err(SyncError::NoResponse { block_id }));
            }
        }

        result
    }

    fn reached_response_ratio(num_of_responses: u32, num_of_expected_responses: u32) -> bool {
        let ratio: f32 =
            ((num_of_responses as f32 / num_of_expected_responses as f32) * 100.0).round();
        ratio >= CERTIFICATE_RESPONSES_RATIO_THRESHOLD * 100.0
    }
}
