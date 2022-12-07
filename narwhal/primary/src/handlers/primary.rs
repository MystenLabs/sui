use crate::metrics::PrimaryMetrics;
use crate::synchronizer::Synchronizer;
use config::{SharedCommittee, SharedWorkerCache, WorkerId};
use crypto::{NetworkPublicKey, PublicKey, Signature};
use dashmap::DashSet;
use fastcrypto::hash::Hash;
use fastcrypto::traits::ToFromBytes;
use fastcrypto::SignatureService;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::{CertificateStore, PayloadToken};
use store::Store;
use tokio::sync::{oneshot, watch};
use tracing::{debug, error, info, warn};
use types::error::{DagError, DagResult};
use types::metered_channel::Sender;
use types::{
    ensure, now, BatchDigest, Certificate, CertificateDigest, FetchCertificatesRequest,
    FetchCertificatesResponse, GetCertificatesRequest, GetCertificatesResponse, Header,
    HeaderDigest, PayloadAvailabilityRequest, PayloadAvailabilityResponse, PrimaryMessage,
    RequestVoteRequest, RequestVoteResponse, Round, Vote, VoteInfo,
};

/// Maximum duration to fetch certficates from local storage.
const FETCH_CERTIFICATES_MAX_HANDLER_TIME: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct PrimaryReceiverController {
    /// The public key of this primary.
    pub name: PublicKey,
    pub committee: SharedCommittee,
    pub worker_cache: SharedWorkerCache,
    pub synchronizer: Arc<Synchronizer>,
    /// Service to sign headers.
    pub signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
    pub tx_certificates: Sender<(Certificate, Option<oneshot::Sender<DagResult<()>>>)>,
    pub header_store: Store<HeaderDigest, Header>,
    pub certificate_store: CertificateStore,
    pub payload_store: Store<(BatchDigest, WorkerId), PayloadToken>,
    /// The store to persist the last voted round per authority, used to ensure idempotence.
    pub vote_digest_store: Store<PublicKey, VoteInfo>,
    /// Get a signal when the round changes.
    pub rx_narwhal_round_updates: watch::Receiver<Round>,
    pub metrics: Arc<PrimaryMetrics>,
    /// Used to ensure a maximum of one inflight vote request per header.
    pub request_vote_inflight: Arc<DashSet<PublicKey>>,
}

#[allow(clippy::result_large_err)]
impl PrimaryReceiverController {
    fn find_next_round(
        &self,
        origin: &PublicKey,
        current_round: Round,
        skip_rounds: &BTreeSet<Round>,
    ) -> Result<Option<Round>, anemo::rpc::Status> {
        let mut current_round = current_round;
        while let Some(round) = self
            .certificate_store
            .next_round_number(origin, current_round)
            .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?
        {
            if !skip_rounds.contains(&round) {
                return Ok(Some(round));
            }
            current_round = round;
        }
        Ok(None)
    }

    #[allow(clippy::mutable_key_type)]
    async fn process_request_vote(
        &self,
        request: anemo::Request<RequestVoteRequest>,
    ) -> DagResult<RequestVoteResponse> {
        let network = request
            .extensions()
            .get::<anemo::NetworkRef>()
            .and_then(anemo::NetworkRef::upgrade)
            .ok_or_else(|| {
                DagError::NetworkError("Unable to access network to send child RPCs".to_owned())
            })?;

        let header = &request.body().header;
        let committee = self.committee.load();
        header.verify(&committee, self.worker_cache.clone())?;

        // Vote request must come from the Header's author.
        let peer_id = request
            .peer_id()
            .ok_or_else(|| DagError::NetworkError("Unable to access remote peer ID".to_owned()))?;
        let peer_network_key = NetworkPublicKey::from_bytes(&peer_id.0).map_err(|e| {
            DagError::NetworkError(format!(
                "Unable to interpret remote peer ID {peer_id:?} as a NetworkPublicKey: {e:?}"
            ))
        })?;
        let (peer_authority, _) = committee
            .authority_by_network_key(&peer_network_key)
            .ok_or_else(|| {
                DagError::NetworkError(format!(
                    "Unable to find authority with network key {peer_network_key:?}"
                ))
            })?;
        ensure!(
            header.author == *peer_authority,
            DagError::NetworkError(format!(
                "Header author {:?} must match requesting peer {peer_authority:?}",
                header.author
            ))
        );

        debug!(
            "Processing vote request for {:?} round:{:?}",
            header, header.round
        );

        // Clone the round updates channel so we can get update notifications specific to
        // this RPC handler.
        let mut rx_narwhal_round_updates = self.rx_narwhal_round_updates.clone();
        // Maximum header age is chosen to strike a balance between allowing for slightly older
        // certificates to still have a chance to be included in the DAG while not wasting
        // resources on very old vote requests. This value affects performance but not correctness
        // of the algorithm.
        const HEADER_AGE_LIMIT: Round = 3;

        // If requester has provided us with parent certificates, process them all
        // before proceeding.
        let mut notifies = Vec::new();
        for certificate in request.body().parents.clone() {
            let (tx_notify, rx_notify) = oneshot::channel();
            notifies.push(rx_notify);
            self.tx_certificates
                .send((certificate, Some(tx_notify)))
                .await
                .map_err(|_| DagError::ChannelFull)?;
        }
        let mut wait_notifies = futures::future::try_join_all(notifies);
        loop {
            tokio::select! {
                results = &mut wait_notifies => {
                    let results: Result<Vec<_>, _> = results
                        .map_err(|e| DagError::ClosedChannel(format!("{e:?}")))?
                        .into_iter()
                        .collect();
                    results?;
                    break
                },
                Ok(_result) = rx_narwhal_round_updates.changed() => {
                    let narwhal_round = *rx_narwhal_round_updates.borrow();
                    ensure!(
                        narwhal_round.saturating_sub(HEADER_AGE_LIMIT) <= header.round,
                        DagError::TooOld(header.digest().into(), header.round, narwhal_round)
                    )
                },
            }
        }

        // Ensure we have the parents. If any are missing, the requester should provide them on retry.
        let (parents, missing) = self.synchronizer.get_parents(header)?;
        if !missing.is_empty() {
            return Ok(RequestVoteResponse {
                vote: None,
                missing,
            });
        }

        // Now that we've got all the required certificates, ensure we're voting on a
        // current Header.
        let narwhal_round = *rx_narwhal_round_updates.borrow();
        ensure!(
            narwhal_round.saturating_sub(HEADER_AGE_LIMIT) <= header.round,
            DagError::TooOld(header.digest().into(), header.round, narwhal_round)
        );

        // Check the parent certificates. Ensure the parents:
        // - form a quorum
        // - are all from the previous round
        // - are from unique authorities
        let mut parent_authorities = BTreeSet::new();
        let mut stake = 0;
        for parent in parents.iter() {
            ensure!(
                parent.round() + 1 == header.round,
                DagError::MalformedHeader(header.digest())
            );
            ensure!(
                parent_authorities.insert(&parent.header.author),
                DagError::MalformedHeader(header.digest())
            );
            stake += committee.stake(&parent.origin());
        }
        ensure!(
            stake >= committee.quorum_threshold(),
            DagError::HeaderRequiresQuorum(header.digest())
        );

        // Synchronize all batches referenced in the header.
        self.synchronizer
            .sync_batches(header, network, /* max_age */ 0)
            .await?;

        // Check that the time of the header is smaller than the current time. If not but the difference is
        // small, just wait. Otherwise reject with an error.
        const TOLERANCE: u64 = 15 * 1000; // 15 sec in milliseconds
        let current_time = now();
        if current_time < header.created_at {
            if header.created_at - current_time < TOLERANCE {
                // for a small difference we simply wait
                tokio::time::sleep(Duration::from_millis(header.created_at - current_time)).await;
            } else {
                // For larger differences return an error, and log it
                warn!(
                    "Rejected header {:?} due to timestamp {} newer than {current_time}",
                    header, header.created_at
                );
                return Err(DagError::InvalidTimestamp {
                    created_time: header.created_at,
                    local_time: current_time,
                });
            }
        }

        // Store the header.
        self.header_store
            .async_write(header.digest(), header.clone())
            .await;

        // Check if we can vote for this header.
        // Send the vote when:
        // 1. when there is no existing vote for this publicKey & epoch/round
        // 2. when there is a vote for this publicKey & epoch/round, and the vote is the same
        // Taking the inverse of these two, the only time we don't want to vote is when:
        // there is a digest for the publicKey & epoch/round, and it does not match the digest
        // of the vote we create for this header.
        // Also when the header is older than one we've already voted for, it is useless to vote,
        // so we don't.
        let result = self
            .vote_digest_store
            .read(header.author.clone())
            .await
            .map_err(DagError::StoreError)?;

        if let Some(vote_info) = result {
            if header.epoch < vote_info.epoch
                || (header.epoch == vote_info.epoch && header.round < vote_info.round)
            {
                // Already voted on a newer Header for this publicKey.
                return Err(DagError::TooOld(
                    header.digest().into(),
                    header.round,
                    narwhal_round,
                ));
            }
            if header.epoch == vote_info.epoch && header.round == vote_info.round {
                // Make sure we don't vote twice for the same authority in the same epoch/round.
                let temp_vote = Vote::new(header, &self.name, &self.signature_service).await;
                if temp_vote.digest() != vote_info.vote_digest {
                    info!(
                        "Authority {} submitted duplicate header for votes at epoch {}, round {}",
                        header.author, header.epoch, header.round
                    );
                    self.metrics
                        .votes_dropped_equivocation_protection
                        .with_label_values(&[&header.epoch.to_string()])
                        .inc();
                    return Err(DagError::AlreadyVoted(vote_info.vote_digest, header.round));
                }
            }
        }

        // Make a vote and send it to the header's creator.
        let vote = Vote::new(header, &self.name, &self.signature_service).await;
        debug!(
            "Created vote {vote:?} for {} at round {}",
            header, header.round
        );

        // Update the vote digest store with the vote we just sent. We don't need to store the
        // vote itself, since it can be reconstructed using the headers.
        self.vote_digest_store
            .sync_write(
                header.author.clone(),
                VoteInfo {
                    epoch: header.epoch,
                    round: header.round,
                    vote_digest: vote.digest(),
                },
            )
            .await?;

        Ok(RequestVoteResponse {
            vote: Some(vote),
            missing: Vec::new(),
        })
    }

    pub async fn send_message(
        &self,
        request: anemo::Request<PrimaryMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        let PrimaryMessage::Certificate(certificate) = request.into_body();
        let (tx_ack, rx_ack) = oneshot::channel();
        self.tx_certificates
            .send((certificate, Some(tx_ack)))
            .await
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;
        rx_ack
            .await
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?
            .map_err(|e| anemo::rpc::Status::internal(e.to_string()))?;
        Ok(anemo::Response::new(()))
    }

    pub async fn request_vote(
        &self,
        request: anemo::Request<RequestVoteRequest>,
    ) -> Result<anemo::Response<RequestVoteResponse>, anemo::rpc::Status> {
        // TODO: Remove manual code for tracking inflight requests once Anemo issue #9 is resolved.
        let author = request.body().header.author.to_owned();
        let _inflight_guard = if self.request_vote_inflight.insert(author.clone()) {
            RequestVoteInflightGuard {
                request_vote_inflight: self.request_vote_inflight.clone(),
                author,
            }
        } else {
            return Err(anemo::rpc::Status::new_with_message(
                // TODO: This should be 429 Too Many Requests, if/when Anemo adds that status code.
                anemo::types::response::StatusCode::Unknown,
                format!("vote request for author {author:?} already inflight"),
            ));
        };

        self.process_request_vote(request)
            .await
            .map(anemo::Response::new)
            .map_err(|e| {
                anemo::rpc::Status::new_with_message(
                    match e {
                        // Report unretriable errors as 400 Bad Request.
                        DagError::InvalidSignature(_)
                        | DagError::InvalidHeaderDigest
                        | DagError::MalformedHeader(_)
                        | DagError::AlreadyVoted(_, _)
                        | DagError::HeaderRequiresQuorum(_)
                        | DagError::TooOld(_, _, _) => {
                            anemo::types::response::StatusCode::BadRequest
                        }
                        // All other errors are retriable.
                        _ => anemo::types::response::StatusCode::Unknown,
                    },
                    format!("{e:?}"),
                )
            })
    }

    pub async fn get_certificates(
        &self,
        request: anemo::Request<GetCertificatesRequest>,
    ) -> Result<anemo::Response<GetCertificatesResponse>, anemo::rpc::Status> {
        let digests = request.into_body().digests;
        if digests.is_empty() {
            return Ok(anemo::Response::new(GetCertificatesResponse {
                certificates: Vec::new(),
            }));
        }

        // TODO [issue #195]: Do some accounting to prevent bad nodes from monopolizing our resources.
        let certificates = self.certificate_store.read_all(digests).map_err(|e| {
            anemo::rpc::Status::internal(format!("error while retrieving certificates: {e}"))
        })?;
        Ok(anemo::Response::new(GetCertificatesResponse {
            certificates: certificates.into_iter().flatten().collect(),
        }))
    }

    pub async fn fetch_certificates(
        &self,
        request: anemo::Request<FetchCertificatesRequest>,
    ) -> Result<anemo::Response<FetchCertificatesResponse>, anemo::rpc::Status> {
        let time_start = Instant::now();
        let peer = request
            .peer_id()
            .map_or_else(|| "None".to_string(), |peer_id| format!("{}", peer_id));
        let request = request.into_body();
        let mut response = FetchCertificatesResponse {
            certificates: Vec::new(),
        };
        if request.max_items == 0 {
            return Ok(anemo::Response::new(response));
        }

        // Use a min-queue for (round, authority) to keep track of the next certificate to fetch.
        //
        // Compared to fetching certificates iteratatively round by round, using a heap is simpler,
        // and avoids the pathological case of iterating through many missing rounds of a downed authority.
        let (lower_bound, skip_rounds) = request.get_bounds();
        debug!(
            "Fetching certificates after round {lower_bound} for peer {:?}, elapsed = {}ms",
            peer,
            time_start.elapsed().as_millis(),
        );

        let mut fetch_queue = BinaryHeap::new();
        for (origin, rounds) in &skip_rounds {
            if rounds.len() > 50 {
                warn!(
                    "{} rounds are available locally for origin {}. elapsed = {}ms",
                    rounds.len(),
                    origin,
                    time_start.elapsed().as_millis(),
                );
            }
            let next_round = self.find_next_round(origin, lower_bound, rounds)?;
            if let Some(r) = next_round {
                fetch_queue.push(Reverse((r, origin.clone())));
            }
        }
        debug!(
            "Initialized origins and rounds to fetch, elapsed = {}ms",
            time_start.elapsed().as_millis(),
        );

        // Iteratively pop the next smallest (Round, Authority) pair, and push to min-heap the next
        // higher round of the same authority that should not be skipped.
        // The process ends when there are no more pairs in the min-heap.
        while let Some(Reverse((round, origin))) = fetch_queue.pop() {
            // Allow the request handler to be stopped after timeout.
            tokio::task::yield_now().await;
            match self
                .certificate_store
                .read_by_index(origin.clone(), round)
                .map_err(|e| anemo::rpc::Status::from_error(Box::new(e)))?
            {
                Some(cert) => {
                    response.certificates.push(cert);
                    let next_round =
                        self.find_next_round(&origin, round, skip_rounds.get(&origin).unwrap())?;
                    if let Some(r) = next_round {
                        fetch_queue.push(Reverse((r, origin.clone())));
                    }
                }
                None => continue,
            };
            if response.certificates.len() == request.max_items {
                debug!(
                    "Collected enough certificates (num={}, elapsed={}ms), returning.",
                    response.certificates.len(),
                    time_start.elapsed().as_millis(),
                );
                break;
            }
            if time_start.elapsed() >= FETCH_CERTIFICATES_MAX_HANDLER_TIME {
                debug!(
                    "Spent enough time reading certificates (num={}, elapsed={}ms), returning.",
                    response.certificates.len(),
                    time_start.elapsed().as_millis(),
                );
                break;
            }
            assert!(response.certificates.len() < request.max_items);
        }

        // The requestor should be able to process certificates returned in this order without
        // any missing parents.
        Ok(anemo::Response::new(response))
    }

    pub async fn get_payload_availability(
        &self,
        request: anemo::Request<PayloadAvailabilityRequest>,
    ) -> Result<anemo::Response<PayloadAvailabilityResponse>, anemo::rpc::Status> {
        let digests = request.into_body().certificate_digests;
        let certificates = self
            .certificate_store
            .read_all(digests.to_owned())
            .map_err(|e| {
                anemo::rpc::Status::internal(format!("error reading certificates: {e:?}"))
            })?;

        let mut result: Vec<(CertificateDigest, bool)> = Vec::new();
        for (id, certificate_option) in digests.into_iter().zip(certificates) {
            // Find batches only for certificates that exist.
            if let Some(certificate) = certificate_option {
                let payload_available = match self
                    .payload_store
                    .read_all(certificate.header.payload)
                    .await
                {
                    Ok(payload_result) => payload_result.into_iter().all(|x| x.is_some()),
                    Err(err) => {
                        // Assume that we don't have the payloads available,
                        // otherwise an error response should be sent back.
                        error!("Error while retrieving payloads: {err}");
                        false
                    }
                };
                result.push((id, payload_available));
            } else {
                // We don't have the certificate available in first place,
                // so we can't even look up the batches.
                result.push((id, false));
            }
        }

        Ok(anemo::Response::new(PayloadAvailabilityResponse {
            payload_availability: result,
        }))
    }
}

// Deletes the tracked inflight request when the RequestVote RPC finishes or is dropped.
struct RequestVoteInflightGuard {
    request_vote_inflight: Arc<DashSet<PublicKey>>,
    author: PublicKey,
}
impl Drop for RequestVoteInflightGuard {
    fn drop(&mut self) {
        assert!(self.request_vote_inflight.remove(&self.author).is_some());
    }
}
