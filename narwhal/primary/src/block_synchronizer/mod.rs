// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    block_synchronizer::{
        peers::Peers,
        PendingIdentifier::{Header, Payload},
    },
    utils,
};
use anemo::PeerId;
use anyhow::anyhow;
use config::{AuthorityIdentifier, Committee, Parameters, WorkerCache, WorkerId};
use crypto::traits::ToFromBytes;
use crypto::NetworkPublicKey;
use fastcrypto::hash::Hash;
use futures::{
    future::{join_all, BoxFuture},
    stream::FuturesUnordered,
    FutureExt, StreamExt,
};
use network::anemo_ext::NetworkExt;
use network::UnreliableNetwork;
use rand::{rngs::SmallRng, SeedableRng};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use storage::{CertificateStore, PayloadStore};
use thiserror::Error;
use tokio::{
    sync::mpsc::{self, Sender},
    task::JoinHandle,
    time::timeout,
};
use tracing::{debug, error, info, instrument, trace, warn};
use types::{
    BatchDigest, Certificate, CertificateDigest, ConditionalBroadcastReceiver,
    GetCertificatesRequest, PayloadAvailabilityRequest, PrimaryToPrimaryClient,
    WorkerSynchronizeMessage,
};

#[cfg(test)]
#[path = "tests/block_synchronizer_tests.rs"]
mod block_synchronizer_tests;

pub mod handler;
pub mod mock;
mod peers;

/// The minimum percentage
/// (number of responses received from primary nodes / number of requests sent to primary nodes)
/// that should be reached when requesting the certificates from peers in order to
/// proceed to next state.
const CERTIFICATE_RESPONSES_RATIO_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone)]
pub struct BlockHeader {
    pub certificate: Certificate,
    /// It designates whether the requested quantity (either the certificate
    /// or the payload) has been retrieved via the local storage. If true,
    /// the it used the storage. If false, then it has been fetched via
    /// the peers.
    pub fetched_from_storage: bool,
}

type ResultSender = Sender<BlockSynchronizeResult<BlockHeader>>;
pub type BlockSynchronizeResult<T> = Result<T, SyncError>;

#[derive(Debug)]
pub enum Command {
    /// A request to synchronize and output the block headers
    /// This will not perform any attempt to fetch the header's
    /// batches. This component does NOT check whether the
    /// requested digests are already synchronized. This is the
    /// consumer's responsibility.
    SynchronizeBlockHeaders {
        digests: Vec<CertificateDigest>,
        respond_to: ResultSender,
    },
    /// A request to synchronize the payload (batches) of the
    /// provided certificates. The certificates are needed in
    /// order to know which batches to ask from the peers
    /// to sync and from which workers.
    /// TODO: We expect to change how batches are stored and
    /// represended (see <https://github.com/MystenLabs/narwhal/issues/54>
    /// and <https://github.com/MystenLabs/narwhal/issues/150> )
    /// and this might relax the requirement to need certificates here.
    ///
    /// This component does NOT check whether the
    //  requested digests are already synchronized. This is the
    //  consumer's responsibility.
    SynchronizeBlockPayload {
        certificates: Vec<Certificate>,
        respond_to: ResultSender,
    },
}

// Those states are used for internal purposes only for the component.
// We are implementing a very very naive state machine and go get from
// one state to the other those commands are being used.
#[allow(clippy::large_enum_variant)]
enum State {
    HeadersSynchronized {
        certificates: HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader>>,
    },
    PayloadAvailabilityReceived {
        certificates: HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader>>,
        peers: Peers<Certificate>,
    },
    PayloadSynchronized {
        result: BlockSynchronizeResult<BlockHeader>,
    },
}

#[derive(Debug, Error, Copy, Clone)]
pub enum SyncError {
    #[error("Certificate with digest {digest} was not returned in any peer response")]
    NoResponse { digest: CertificateDigest },

    #[error(
        "Certificate with digest {digest} could not be retrieved, timeout while retrieving result"
    )]
    Timeout { digest: CertificateDigest },
}

impl SyncError {
    #[allow(dead_code)]
    pub fn digest(&self) -> CertificateDigest {
        match *self {
            SyncError::NoResponse { digest } | SyncError::Timeout { digest } => digest,
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
    fn digest(&self) -> CertificateDigest {
        match self {
            Header(digest) | Payload(digest) => *digest,
        }
    }
}

pub struct BlockSynchronizer {
    /// The id of this primary.
    authority_id: AuthorityIdentifier,

    /// The committee information.
    committee: Committee,

    /// The worker information cache.
    worker_cache: WorkerCache,

    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,

    /// Receive the commands for the synchronizer
    rx_block_synchronizer_commands: mpsc::Receiver<Command>,

    /// Pending block requests either for header or payload type
    pending_requests: HashMap<PendingIdentifier, Vec<ResultSender>>,

    /// Send network requests
    network: anemo::Network,

    /// The store that holds the certificates
    certificate_store: CertificateStore,

    /// The persistent storage for payload markers from workers
    payload_store: PayloadStore,

    /// Timeout when synchronizing the certificates
    certificates_synchronize_timeout: Duration,

    /// Timeout when synchronizing the payload
    payload_synchronize_timeout: Duration,

    /// Timeout when has requested the payload and waiting to receive
    payload_availability_timeout: Duration,
}

impl BlockSynchronizer {
    #[must_use]
    pub fn spawn(
        authority_id: AuthorityIdentifier,
        committee: Committee,
        worker_cache: WorkerCache,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_block_synchronizer_commands: mpsc::Receiver<Command>,
        network: anemo::Network,
        payload_store: PayloadStore,
        certificate_store: CertificateStore,
        parameters: Parameters,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let _ = &parameters;
            Self {
                name,
                committee,
                worker_cache,
                rx_shutdown,
                rx_block_synchronizer_commands,
                pending_requests: HashMap::new(),
                network,
                payload_store,
                certificate_store,
                certificates_synchronize_timeout: parameters
                    .block_synchronizer
                    .certificates_synchronize_timeout,
                payload_synchronize_timeout: parameters
                    .block_synchronizer
                    .payload_availability_timeout,
                payload_availability_timeout: parameters
                    .block_synchronizer
                    .payload_availability_timeout,
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

        info!(
            "BlockSynchronizer on node {} has started successfully.",
            self.authority_id
        );
        loop {
            tokio::select! {
                Some(command) = self.rx_block_synchronizer_commands.recv() => {
                    match command {
                        Command::SynchronizeBlockHeaders { digests, respond_to } => {
                            let fut = self.handle_synchronize_block_headers_command(digests, respond_to).await;
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
                Some(state) = waiting.next() => {
                    match state {
                        State::HeadersSynchronized { certificates } => {
                            debug!("Result for the block headers synchronize request with certs {certificates:?}");

                            for (digest, result) in certificates {
                                self.notify_requestors_for_result(Header(digest), result).await;
                            }
                        },
                        State::PayloadAvailabilityReceived { certificates, peers } => {
                             debug!("Result for the block payload synchronize request wwith certs {certificates:?}");

                            // now try to synchronise the payload only for the ones that have been found
                            let futures = self.handle_synchronize_block_payloads(peers).await;
                            for fut in futures {
                                waiting.push(fut);
                            }

                            // notify immediately for digests that have been errored or timedout
                            for (digest, result) in certificates {
                                if result.is_err() {
                                    self.notify_requestors_for_result(Payload(digest), result).await;
                                }
                            }
                        },
                        State::PayloadSynchronized { result } => {
                            let digest = result.as_ref().map_or_else(|e| e.digest(), |r| r.certificate.digest());

                            debug!("Block payload synchronize result received for certificate digest {digest}");

                            self.notify_requestors_for_result(Payload(digest), result).await;
                        },
                    }
                }

                _ = self.rx_shutdown.receiver.recv() => {
                    return
                }

            }
        }
    }

    #[instrument(level = "trace", skip_all)]
    async fn notify_requestors_for_result(
        &mut self,
        request: PendingIdentifier,
        result: BlockSynchronizeResult<BlockHeader>,
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
        respond_to: ResultSender,
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
    #[instrument(level="trace", skip_all, fields(num_certificates = certificates.len()))]
    async fn handle_synchronize_block_payload_command<'a>(
        &mut self,
        certificates: Vec<Certificate>,
        respond_to: ResultSender,
    ) -> Option<BoxFuture<'a, State>> {
        let mut certificates_to_sync = Vec::new();
        let mut digests_to_sync = Vec::new();

        let missing_certificates = self
            .reply_with_payload_already_in_storage(certificates.clone(), respond_to.clone())
            .await;

        for certificate in missing_certificates {
            let block_id = certificate.digest();

            if self.resolve_pending_request(Payload(block_id), respond_to.clone()) {
                certificates_to_sync.push(certificate);
                digests_to_sync.push(block_id);
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

        // TODO: add metric here to track the number of certificates
        // requested that are missing a payload

        let request = PayloadAvailabilityRequest {
            certificate_digests: digests_to_sync,
        };
        let primaries: Vec<_> = self
            .committee
            .others_primaries_by_id(self.authority_id)
            .into_iter()
            .map(|(_name, _address, network_key)| network_key)
            .collect();

        // Now create the future that will send the requests.
        let timeout = self.payload_availability_timeout;
        let network = self.network.clone();
        Some(
            Self::send_payload_availability_requests(
                timeout,
                certificates_to_sync,
                request,
                primaries,
                network,
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
    #[instrument(level = "trace", skip_all)]
    async fn handle_synchronize_block_headers_command<'a>(
        &mut self,
        block_ids: Vec<CertificateDigest>,
        respond_to: ResultSender,
    ) -> Option<BoxFuture<'a, State>> {
        let missing_block_ids = self
            .reply_with_certificates_already_in_storage(block_ids.clone(), respond_to.clone())
            .await;

        // check if there are pending requests on the block_ids.
        // If yes, then ignore.
        let mut to_sync = Vec::new();
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

        // Create a future to broadcast certificate requests.
        let network_keys: Vec<_> = self
            .committee
            .others_primaries_by_id(self.authority_id)
            .into_iter()
            .map(|(_name, _address, network_key)| network_key)
            .collect();
        let network = self.network.clone();
        let timeout = self.certificates_synchronize_timeout;
        let committee = self.committee.clone();
        let worker_cache = self.worker_cache.clone();
        Some(
            Self::send_certificate_requests(
                network,
                network_keys,
                timeout,
                committee,
                worker_cache,
                to_sync,
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
        respond_to: Sender<BlockSynchronizeResult<BlockHeader>>,
    ) -> HashSet<CertificateDigest> {
        // find the certificates that already exist in storage
        match self.certificate_store.read_all(block_ids.clone()) {
            Ok(certificates) => {
                let (found, missing): (
                    Vec<(CertificateDigest, Option<Certificate>)>,
                    Vec<(CertificateDigest, Option<Certificate>)>,
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
    #[instrument(level = "trace", skip_all, fields(num_certificates = certificates.len()))]
    async fn reply_with_payload_already_in_storage(
        &self,
        certificates: Vec<Certificate>,
        respond_to: Sender<BlockSynchronizeResult<BlockHeader>>,
    ) -> Vec<Certificate> {
        let mut missing_payload_certs = Vec::new();
        let mut futures = Vec::new();

        for certificate in certificates {
            let payload: Vec<(BatchDigest, WorkerId)> = certificate
                .header()
                .payload()
                .clone()
                .into_iter()
                .map(|(batch, (worker_id, _))| (batch, worker_id))
                .collect();

            let payload_available = if certificate.header().author() == self.authority_id {
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
                match self.payload_store.read_all(payload) {
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

    #[instrument(level = "trace", skip_all)]
    async fn handle_synchronize_block_payloads<'a>(
        &mut self,
        mut peers: Peers<Certificate>,
    ) -> Vec<BoxFuture<'a, State>> {
        // Rebalance the CertificateDigests to ensure that
        // those are uniquely distributed across the peers.
        peers.rebalance_values();

        for peer in peers.peers().values() {
            let target = match self.committee.authority_by_network_key(&peer.name) {
                Some(authority) => authority.id(),
                None => {
                    error!(
                        "could not look up authority for network key {:?}",
                        peer.name
                    );
                    continue;
                }
            };
            self.send_synchronize_payload_requests(target, peer.assigned_values())
                .await
        }

        peers
            .unique_values()
            .into_iter()
            .map(|certificate| {
                let timeout = self.payload_synchronize_timeout;
                let payload_store = self.payload_store.clone();
                Self::wait_for_block_payload(timeout, payload_store, certificate).boxed()
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
    #[instrument(level = "trace", skip_all, fields(peer_name = ?primary_peer_name, num_certificates = certificates.len()))]
    async fn send_synchronize_payload_requests(
        &mut self,
        primary_peer_name: AuthorityIdentifier,
        certificates: Vec<Certificate>,
    ) {
        let batches_by_worker = utils::map_certificate_batches_by_worker(certificates.as_slice());

        for (worker_id, batch_ids) in batches_by_worker {
            let worker_name = self
                .worker_cache
                .worker(
                    self.committee
                        .authority(&self.authority_id)
                        .unwrap()
                        .protocol_key(),
                    &worker_id,
                )
                .expect("Worker id not found")
                .name;

            let message = WorkerSynchronizeMessage {
                digests: batch_ids,
                target: primary_peer_name,
                is_certified: true,
            };
            let _ = self.network.unreliable_send(worker_name, &message);

            debug!(
                "Sent request for batch ids {:?} to worker id {}",
                message.digests, worker_id
            );
        }
    }

    #[instrument(level = "trace", skip_all, fields(request_id, certificate=?certificate.header().digest()))]
    async fn wait_for_block_payload<'a>(
        payload_synchronize_timeout: Duration,
        payload_store: PayloadStore,
        certificate: Certificate,
    ) -> State {
        let futures = certificate
            .header()
            .payload()
            .iter()
            .map(|(batch_digest, (worker_id, _))| {
                payload_store.notify_contains(*batch_digest, *worker_id)
            })
            .collect::<Vec<_>>();

        // Wait for all the items to sync - have a timeout
        let result = timeout(payload_synchronize_timeout, join_all(futures)).await;
        if result.is_err() {
            return State::PayloadSynchronized {
                result: Err(SyncError::Timeout {
                    digest: certificate.digest(),
                }),
            };
        }

        State::PayloadSynchronized {
            result: Ok(BlockHeader {
                certificate,
                fetched_from_storage: false,
            }),
        }
    }

    async fn send_certificate_requests(
        network: anemo::Network,
        targets: Vec<NetworkPublicKey>,
        timeout: Duration,
        committee: Committee,
        worker_cache: WorkerCache,
        digests: Vec<CertificateDigest>,
    ) -> State {
        let request = GetCertificatesRequest {
            digests: digests.clone(),
        };
        let mut requests: FuturesUnordered<_> = targets
            .iter()
            .map(|target| {
                let network = network.clone();
                let request = anemo::Request::new(request.clone()).with_timeout(timeout);
                async move {
                    let peer_id = PeerId(target.0.to_bytes());
                    let peer = network.peer(peer_id).ok_or_else(|| {
                        anemo::rpc::Status::internal(format!(
                            "Network has no connection with peer {peer_id}"
                        ))
                    })?;
                    PrimaryToPrimaryClient::new(peer)
                        .get_certificates(request)
                        .await
                }
            })
            .collect();

        let total_expected_certificates = digests.len();
        let mut num_of_responses: u32 = 0;
        let num_of_requests_sent: u32 = targets.len() as u32;

        let mut peers = Peers::<Certificate>::new(SmallRng::from_entropy());

        while let Some(result) = requests.next().await {
            num_of_responses += 1;

            let response = match result {
                Ok(response) => response,
                Err(e) => {
                    info!(
                        "GetCertificates request to peer {:?} failed: {e:?}",
                        e.peer_id()
                    );
                    continue;
                }
            };

            let response_peer = match response
                .peer_id()
                .ok_or_else(|| anyhow!("missing peer_id"))
                .and_then(|id| NetworkPublicKey::from_bytes(&id.0).map_err(|e| e.into()))
            {
                Ok(peer) => peer,
                Err(e) => {
                    error!("Could not extract peer from GetCertificates response: {e:?}");
                    continue;
                }
            };

            if peers.contains_peer(&response_peer) {
                // skip , we already got an answer from this peer
                continue;
            }

            let certificates = &response.body().certificates;
            let mut found_invalid_certificate = false;
            for certificate in certificates {
                if let Err(err) = certificate.verify(&committee, &worker_cache) {
                    error!(
                        "Ignoring certificates from peer {response_peer:?}: certificate verification failed for digest {} with error {err:?}",
                        certificate.digest(),
                    );
                    found_invalid_certificate = true;
                }
            }
            if found_invalid_certificate {
                continue;
            }

            // Ensure we got responses for the certificates we asked for.
            // Even if we have found one certificate that doesn't match
            // we reject the payload - it shouldn't happen.
            if certificates.iter().any(|c| !digests.contains(&c.digest())) {
                warn!("Ignoring certificates form peer {response_peer:?}: found at least one that we haven't asked for");
                continue;
            }

            // Add them as a new peer.
            peers.add_peer(response_peer.clone(), response.into_body().certificates);

            if (peers.unique_value_count() == total_expected_certificates
                && Self::reached_response_ratio(num_of_responses, num_of_requests_sent))
                || num_of_responses == num_of_requests_sent
            {
                // We have received enough responses.
                return State::HeadersSynchronized {
                    certificates: Self::resolve_block_synchronize_result(&peers, digests, false),
                };
            }
        }

        // Return whatever we have.
        State::HeadersSynchronized {
            certificates: Self::resolve_block_synchronize_result(&peers, digests, true),
        }
    }

    async fn send_payload_availability_requests(
        fetch_certificates_timeout: Duration,
        certificates: Vec<Certificate>,
        request: PayloadAvailabilityRequest,
        primaries: Vec<NetworkPublicKey>,
        network: anemo::Network,
    ) -> State {
        let total_expected_block_ids = certificates.len();
        let mut num_of_responses: u32 = 0;
        let num_of_requests_sent: u32 = primaries.len() as u32;
        let certificates_by_id: HashMap<CertificateDigest, Certificate> = certificates
            .iter()
            .map(|c| (c.digest(), c.clone()))
            .collect();
        let block_ids: Vec<CertificateDigest> =
            certificates_by_id.keys().map(|id| id.to_owned()).collect();

        let get_payload_availability_fn =
            move |mut client: PrimaryToPrimaryClient<network::anemo_ext::WaitingPeer>, request| {
                // Wrapper function enables us to move `client` into the future.
                async move { client.get_payload_availability(request).await }
            };
        let mut requests: FuturesUnordered<_> = primaries
            .iter()
            .map(|name| {
                let id = anemo::PeerId(name.0.to_bytes());
                let peer = network.waiting_peer(id);
                let request =
                    anemo::Request::new(request.clone()).with_timeout(fetch_certificates_timeout);
                get_payload_availability_fn(PrimaryToPrimaryClient::new(peer), request)
            })
            .collect();
        let mut peers = Peers::<Certificate>::new(SmallRng::from_entropy());

        while let Some(result) = requests.next().await {
            num_of_responses += 1;

            let response = match result {
                Ok(response) => response,
                Err(e) => {
                    info!(
                        "GetPayloadAvailability request to peer {:?} failed: {e:?}",
                        e.peer_id()
                    );
                    continue;
                }
            };

            let response_peer = match response
                .peer_id()
                .ok_or_else(|| anyhow!("missing peer_id"))
                .and_then(|id| NetworkPublicKey::from_bytes(&id.0).map_err(|e| e.into()))
            {
                Ok(peer) => peer,
                Err(e) => {
                    info!("Could not extract peer from GetPayloadAvailability response: {e:?}");
                    continue;
                }
            };

            if peers.contains_peer(&response_peer) {
                // skip , we already got an answer from this peer
                continue;
            }

            // Ensure we got responses for the certificates we asked for.
            // Even if we have found one certificate that doesn't match
            // we reject the payload - it shouldn't happen. Also, add the
            // found ones in a vector.
            let mut available_certs_for_peer = Vec::new();
            for id in response.body().available_certificates() {
                if let Some(c) = certificates_by_id.get(&id) {
                    available_certs_for_peer.push(c.clone());
                } else {
                    // We should expect to have found every
                    // responded id to our list of certificates.
                    continue;
                }
            }

            // add them as a new peer
            peers.add_peer(response_peer.clone(), available_certs_for_peer);

            // We have received all possible responses
            if (peers.unique_value_count() == total_expected_block_ids
                && Self::reached_response_ratio(num_of_responses, num_of_requests_sent))
                || num_of_responses == num_of_requests_sent
            {
                let result = Self::resolve_block_synchronize_result(&peers, block_ids, false);

                return State::PayloadAvailabilityReceived {
                    certificates: result,
                    peers,
                };
            }
        }
        let result = Self::resolve_block_synchronize_result(&peers, block_ids, true);

        State::PayloadAvailabilityReceived {
            certificates: result,
            peers,
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
        peers: &Peers<Certificate>,
        block_ids: Vec<CertificateDigest>,
        timeout: bool,
    ) -> HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader>> {
        let mut certificates_by_id: HashMap<CertificateDigest, Certificate> = peers
            .unique_values()
            .into_iter()
            .map(|c| (c.digest(), c))
            .collect();

        let mut result: HashMap<CertificateDigest, BlockSynchronizeResult<BlockHeader>> =
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
                result.insert(block_id, Err(SyncError::Timeout { digest: block_id }));
            } else {
                result.insert(block_id, Err(SyncError::NoResponse { digest: block_id }));
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
