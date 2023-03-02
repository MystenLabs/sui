// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    aggregators::{CertificatesAggregator, VotesAggregator},
    certificate_fetcher::CertificateLoopbackMessage,
    primary::PrimaryMessage,
    synchronizer::Synchronizer,
};

use anyhow::Result;
use config::{Committee, Epoch};
use crypto::{NetworkPublicKey, PublicKey, Signature};
use fastcrypto::{hash::Hash as _, signature_service::SignatureService};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use network::{anemo_ext::NetworkExt, CancelOnDropHandler, ReliableNetwork};
use std::time::Duration;
use std::{collections::HashMap, sync::Arc, time::Instant};
use storage::CertificateStore;
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        oneshot, watch,
    },
    task::{JoinHandle, JoinSet},
};
use tracing::{debug, enabled, error, info, instrument, trace, warn};
use types::{
    ensure,
    error::{DagError, DagResult},
    Certificate, CertificateDigest, ConditionalBroadcastReceiver, Header, HeaderDigest,
    PrimaryToPrimaryClient, RequestVoteRequest, Round, Vote,
};

#[cfg(test)]
#[path = "tests/core_tests.rs"]
pub mod core_tests;

pub struct Core {
    /// The public key of this primary.
    name: PublicKey,
    /// The committee information.
    committee: Committee,
    /// The persistent storage keyed to headers.
    header_store: Store<HeaderDigest, Header>,
    /// The persistent storage keyed to certificates.
    certificate_store: CertificateStore,
    /// Handles synchronization with other nodes and our workers.
    synchronizer: Arc<Synchronizer>,
    /// Service to sign headers.
    signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
    /// Get a signal when the consensus round changes
    rx_consensus_round_updates: watch::Receiver<Round>,
    /// Get a signal when the narwhal round changes
    rx_narwhal_round_updates: watch::Receiver<Round>,
    /// The depth of the garbage collector.
    gc_depth: Round,

    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Receiver for certificates.
    rx_certificates: Receiver<(Certificate, Option<oneshot::Sender<DagResult<()>>>)>,
    /// Receives loopback certificates from the `CertificateFetcher`.
    rx_certificates_loopback: Receiver<CertificateLoopbackMessage>,
    /// Receives our newly created headers from the `Proposer`.
    rx_headers: Receiver<Header>,
    /// Output all certificates to the consensus layer.
    tx_new_certificates: Sender<Certificate>,
    /// Send valid a quorum of certificates' ids to the `Proposer` (along with their round).
    tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,

    /// The last garbage collected round.
    gc_round: Round,
    /// The highest certificates round received by this node.
    highest_received_round: Round,
    /// The highest certificates round processed by this node.
    highest_processed_round: Round,
    /// Certificates awaiting processing due to missing ancestors.
    pending_certificates: HashMap<CertificateDigest, Vec<oneshot::Sender<DagResult<()>>>>,
    /// Contains background tasks for:
    /// - synchronizing worker batches for processed certificates
    /// - broadcasting newly formed certificates
    background_tasks: JoinSet<DagResult<()>>,
    /// Used to cancel vote requests for a previously-proposed header that is being replaced
    /// before a certificate could be formed.
    cancel_proposed_header: Option<oneshot::Sender<()>>,
    /// Handle to propose_header task. Our target is to have only one task running always, thus
    /// we cancel the previously running before we spawn the next one. However, we don't wait for
    /// the previous to finish to spawn the new one, so we might temporarily have more that one
    /// parallel running, which should be fine though.
    propose_header_tasks: JoinSet<DagResult<Certificate>>,
    /// Aggregates certificates to use as parents for new headers.
    certificates_aggregators: HashMap<Round, Box<CertificatesAggregator>>,
    /// A network sender to send the batches to the other workers.
    network: anemo::Network,
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        header_store: Store<HeaderDigest, Header>,
        certificate_store: CertificateStore,
        synchronizer: Arc<Synchronizer>,
        signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
        rx_consensus_round_updates: watch::Receiver<Round>,
        rx_narwhal_round_updates: watch::Receiver<Round>,
        gc_depth: Round,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_certificates: Receiver<(Certificate, Option<oneshot::Sender<DagResult<()>>>)>,
        rx_certificates_loopback: Receiver<CertificateLoopbackMessage>,
        rx_headers: Receiver<Header>,
        tx_new_certificates: Sender<Certificate>,
        tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,
        primary_network: anemo::Network,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                header_store,
                certificate_store,
                synchronizer,
                signature_service,
                rx_consensus_round_updates,
                rx_narwhal_round_updates,
                gc_depth,
                rx_shutdown,
                rx_certificates,
                rx_certificates_loopback,
                rx_headers,
                tx_new_certificates,
                tx_parents,
                gc_round: 0,
                highest_received_round: 0,
                highest_processed_round: 0,
                pending_certificates: HashMap::new(),
                background_tasks: JoinSet::new(),
                cancel_proposed_header: None,
                propose_header_tasks: JoinSet::new(),
                certificates_aggregators: HashMap::with_capacity(2 * gc_depth as usize),
                network: primary_network,
            }
            .run_inner()
            .await
        })
    }

    #[instrument(level = "info", skip_all)]
    async fn run_inner(self) {
        let core = async move { self.recover().await?.run().await };

        match core.await {
            Err(err @ DagError::ShuttingDown) => debug!("{:?}", err),
            Err(err) => panic!("{:?}", err),
            Ok(_) => {}
        }
    }

    #[instrument(level = "info", skip_all)]
    pub async fn recover(mut self) -> DagResult<Self> {
        info!("Starting certificate recovery. Message processing will begin after completion.");

        let last_round_certificates = self
            .certificate_store
            .last_two_rounds_certs()
            .expect("Failed recovering certificates in primary core");

        let last_round_number = last_round_certificates
            .first()
            .map(|c| c.round())
            .unwrap_or(0);

        for certificate in last_round_certificates {
            self.append_certificate_in_aggregator(certificate).await?;
        }

        self.highest_received_round = last_round_number;
        self.highest_processed_round = last_round_number;

        Ok(self)
    }

    // Requests a vote for a Header from the given peer. Retries indefinitely until either a
    // vote is received, or a permanent error is returned.
    #[instrument(level = "debug", skip_all, fields(header_digest = ?header.digest()))]
    async fn request_vote(
        network: anemo::Network,
        committee: Committee,
        certificate_store: CertificateStore,
        authority: PublicKey,
        target: NetworkPublicKey,
        header: Header,
    ) -> DagResult<Vote> {
        let peer_id = anemo::PeerId(target.0.to_bytes());
        let peer = network.waiting_peer(peer_id);

        fail::fail_point!("request-vote", |_| {
            Err(DagError::NetworkError(format!(
                "Injected error in request vote for {header}"
            )))
        });

        let mut client = PrimaryToPrimaryClient::new(peer);

        let mut missing_parents: Vec<CertificateDigest> = Vec::new();
        let mut attempt: u32 = 0;
        let vote = loop {
            attempt += 1;

            let parents = if missing_parents.is_empty() {
                Vec::new()
            } else {
                let expected_count = missing_parents.len();
                let parents: Vec<_> = certificate_store
                    .read_all(
                        missing_parents
                            .into_iter()
                            // Only provide certs that are parents for the requested vote.
                            .filter(|parent| header.parents.contains(parent)),
                    )?
                    .into_iter()
                    .flatten()
                    .collect();
                if parents.len() != expected_count {
                    warn!("tried to read {expected_count} missing certificates requested by remote primary for vote request, but only found {}", parents.len());
                    return Err(DagError::ProposedHeaderMissingCertificates);
                }
                parents
            };

            // TODO: Remove timeout from this RPC once anemo issue #10 is resolved.
            match client
                .request_vote(RequestVoteRequest {
                    header: header.clone(),
                    parents,
                })
                .await
            {
                Ok(response) => {
                    let response = response.into_body();
                    if response.vote.is_some() {
                        break response.vote.unwrap();
                    }
                    missing_parents = response.missing;
                }
                Err(status) => {
                    if status.status() == anemo::types::response::StatusCode::BadRequest {
                        return Err(DagError::NetworkError(format!(
                            "unrecoverable error requesting vote for {header}: {status:?}"
                        )));
                    }
                    missing_parents = Vec::new();
                }
            }

            // Retry delay. Using custom values here because pure exponential backoff is hard to
            // configure without it being either too aggressive or too slow. We want the first
            // retry to be instantaneous, next couple to be fast, and to slow quickly thereafter.
            tokio::time::sleep(Duration::from_millis(match attempt {
                1 => 0,
                2 => 100,
                3 => 500,
                4 => 1_000,
                5 => 2_000,
                6 => 5_000,
                _ => 10_000,
            }))
            .await;
        };

        // Verify the vote.
        ensure!(
            header.round == vote.round,
            DagError::InvalidRound {
                expected: header.round,
                received: vote.round
            }
        );
        ensure!(
            vote.digest == header.digest()
                && vote.origin == header.author
                && vote.author == authority,
            DagError::UnexpectedVote(vote.digest)
        );
        vote.verify(&committee)?;
        Ok(vote)
    }

    #[instrument(level = "debug", skip_all, fields(header_digest = ?header.digest()))]
    async fn propose_header(
        name: PublicKey,
        committee: Committee,
        header_store: Store<HeaderDigest, Header>,
        certificate_store: CertificateStore,
        signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
        network: anemo::Network,
        header: Header,
        mut cancel: oneshot::Receiver<()>,
    ) -> DagResult<Certificate> {
        if header.epoch != committee.epoch() {
            debug!(
                "Core received mismatched header proposal for epoch {}, currently at epoch {}",
                header.epoch,
                committee.epoch()
            );
            return Err(DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: header.epoch,
            });
        }

        // Process the header.
        header_store
            .async_write(header.digest(), header.clone())
            .await;

        // TODO(metrics): Set `proposed_header_round` to `header.round as i64`.

        // Reset the votes aggregator and sign our own header.
        let mut votes_aggregator = VotesAggregator::new();
        let vote = Vote::new(&header, &name, &signature_service).await;
        let mut certificate = votes_aggregator.append(vote, &committee, &header)?;

        // Trigger vote requests.
        let peers = committee
            .others_primaries(&name)
            .into_iter()
            .map(|(name, _, network_key)| (name, network_key));
        let mut requests: FuturesUnordered<_> = peers
            .map(|(name, target)| {
                let header = header.clone();
                Self::request_vote(
                    network.clone(),
                    committee.clone(),
                    certificate_store.clone(),
                    name,
                    target,
                    header,
                )
            })
            .collect();
        loop {
            if certificate.is_some() {
                break;
            }
            tokio::select! {
                result = &mut requests.next() => {
                    match result {
                        Some(Ok(vote)) => {
                            certificate = votes_aggregator.append(
                                vote,
                                &committee,
                                &header,
                            )?;
                        },
                        Some(Err(e)) => debug!("failed to get vote for header {header:?}: {e:?}"),
                        None => break,
                    }
                },
                _ = &mut cancel => {
                    debug!("canceling Header proposal {header} for round {}", header.round);
                    return Err(DagError::Canceled)
                },
            }
        }

        let certificate = certificate.ok_or_else(|| {
            // Log detailed header info if we failed to form a certificate.
            if enabled!(tracing::Level::WARN) {
                let mut msg = format!(
                    "Failed to form certificate from header {header:?} with parent certificates:\n"
                );
                for parent_digest in header.parents.iter() {
                    let parent_msg = match certificate_store.read(*parent_digest) {
                        Ok(Some(cert)) => format!("{cert:?}\n"),
                        Ok(None) => {
                            format!("!!!missing certificate for digest {parent_digest:?}!!!\n")
                        }
                        Err(e) => format!(
                            "!!!error retrieving certificate for digest {parent_digest:?}: {e:?}\n"
                        ),
                    };
                    msg.push_str(&parent_msg);
                }
                warn!(msg);
            }
            DagError::CouldNotFormCertificate(header.digest())
        })?;
        debug!("Assembled {certificate:?}");

        Ok(certificate)
    }

    // Awaits completion of the given certificate broadcasts, aborting if narwhal round
    // advances past certificate round.
    #[instrument(level = "debug", skip_all, fields(certificate_round = certificate_round))]
    async fn send_certificates_while_current(
        certificate_round: Round,
        tasks: Vec<CancelOnDropHandler<Result<anemo::Response<()>>>>,
        mut rx_narwhal_round_updates: watch::Receiver<Round>,
    ) -> DagResult<()> {
        let mut narwhal_round = *rx_narwhal_round_updates.borrow();
        if narwhal_round > certificate_round {
            return Ok(());
        }

        let mut join_all = futures::future::try_join_all(tasks);
        loop {
            tokio::select! {
                _ = &mut join_all => {
                    // Reliable broadcast will not return errors.
                    return Ok(())
                },
                result = rx_narwhal_round_updates.changed() => {
                    if result.is_err() {
                        // this happens during reconfig when the other side hangs up.
                        return Ok(());
                    }
                    narwhal_round = *rx_narwhal_round_updates.borrow();
                    if narwhal_round > certificate_round {
                        // Round has advanced. No longer need to broadcast this cert to
                        // ensure liveness.
                        return Ok(())
                    }
                },
            }
        }
    }

    #[instrument(level = "debug", skip_all, fields(certificate_digest = ?certificate.digest()))]
    async fn process_own_certificate(&mut self, certificate: Certificate) -> DagResult<()> {
        // Process the new certificate.
        match self.process_certificate_internal(certificate.clone()).await {
            Ok(()) => Ok(()),
            result @ Err(DagError::ShuttingDown) => result,
            Err(e) => panic!("Failed to process locally-created certificate: {e}"),
        }?;

        // Broadcast the certificate.
        let round = certificate.header.round;
        let header_to_certificate_duration =
            Duration::from_millis(certificate.metadata.created_at - certificate.header.created_at)
                .as_secs_f64();
        let network_keys = self
            .committee
            .others_primaries(&self.name)
            .into_iter()
            .map(|(_, _, network_key)| network_key)
            .collect();
        let tasks = self.network.broadcast(
            network_keys,
            &PrimaryMessage::Certificate(certificate.clone()),
        );
        self.background_tasks
            .spawn(Self::send_certificates_while_current(
                round,
                tasks,
                self.rx_narwhal_round_updates.clone(),
            ));

        // Update metrics.
        // TODO(metrics): Set `certificate_created_round` to `round as i64`
        // TODO(metrics): Increment `certificates_created` by 1
        // TODO(metrics): Observe `header_to_certificate_duration` as `header_to_certificate_latency`

        // NOTE: This log entry is used to compute performance.
        debug!(
            "Header {:?} took {} seconds to be materialized to a certificate {:?}",
            certificate.header.digest(),
            header_to_certificate_duration,
            certificate.digest()
        );

        Ok(())
    }

    /// Checks if all parents of the certificate exist, such that the certificate can be added to
    /// header parents and inserted into the dag.
    /// The certificate must have been verified.
    #[instrument(level = "debug", skip_all, fields(certificate_digest = ?certificate.digest()))]
    async fn process_certificate(
        &mut self,
        certificate: Certificate,
        notify: Option<oneshot::Sender<DagResult<()>>>,
    ) -> DagResult<()> {
        let digest = certificate.digest();
        match self.process_certificate_internal(certificate).await {
            Err(DagError::Suspended) => {
                if let Some(notify) = notify {
                    self.pending_certificates
                        .entry(digest)
                        .or_insert_with(Vec::new)
                        .push(notify);
                    Ok(())
                } else {
                    Err(DagError::Suspended)
                }
            }
            result => {
                if let Some(notify) = notify {
                    let _ = notify.send(result.clone()); // no problem if remote side isn't listening
                }
                if let Some(notifies) = self.pending_certificates.remove(&digest) {
                    for notify in notifies {
                        let _ = notify.send(result.clone()); // no problem if remote side isn't listening
                    }
                }
                result
            }
        }
    }

    #[instrument(level = "debug", skip_all, fields(certificate_digest = ?certificate.digest()))]
    async fn process_certificate_internal(&mut self, certificate: Certificate) -> DagResult<()> {
        // Ok to ignore a certificate early than gc_round, because it will never be included into
        // the consensus dag.
        ensure!(
            self.gc_round < certificate.round(),
            DagError::TooOld(
                certificate.digest().into(),
                certificate.round(),
                self.gc_round
            )
        );
        let digest = certificate.digest();
        if self.certificate_store.contains(&digest)? {
            trace!("Certificate {digest:?} has already been processed. Skip processing.");
            // TODO(metrics): Increment `duplicate_certificates_processed` by 1
            return Ok(());
        }
        debug!(
            "Processing certificate {:?} round:{:?}",
            certificate,
            certificate.round()
        );

        let _certificate_source = if self.name.eq(&certificate.header.author) {
            "own"
        } else {
            "other"
        };
        self.highest_received_round = self.highest_received_round.max(certificate.round());
        // TODO(metrics): Set `highest_received_round` to `self.highest_received_round as i64`

        // Let the proposer draw early conclusions from a certificate at this round and epoch, without its
        // parents or payload (which we may not have yet).
        //
        // Since our certificate is well-signed, it shows a majority of honest signers stand at round r,
        // so to make a successful proposal, our proposer must use parents at least at round r-1.
        //
        // This allows the proposer not to fire proposals at rounds strictly below the certificate we witnessed.
        let minimal_round_for_parents = certificate.round().saturating_sub(1);
        self.tx_parents
            .send((vec![], minimal_round_for_parents, certificate.epoch()))
            .await
            .map_err(|_| DagError::ShuttingDown)?;

        // Instruct workers to download any missing batches referenced in this certificate.
        // Since this header got certified, we are sure that all the data it refers to (ie. its batches and its parents) are available.
        // We can thus continue the processing of the certificate without blocking on batch synchronization.
        let synchronizer = self.synchronizer.clone();
        let header = certificate.header.clone();
        let network = self.network.clone();
        let max_age = self.gc_depth.saturating_sub(1);
        self.background_tasks
            .spawn(async move { synchronizer.sync_batches(&header, network, max_age).await });

        // Ensure either we have all the ancestors of this certificate, or the parents have been garbage collected.
        // If we don't, the synchronizer will start fetching missing certificates.
        if certificate.round() > self.gc_round + 1
            && !self.synchronizer.check_parents(&certificate).await?
        {
            debug!(
                "Processing certificate {:?} suspended: missing ancestors",
                certificate
            );
            // TODO(metrics): Increment `certificates_suspended` by 1
            return Err(DagError::Suspended);
        }

        // Store the certificate. Afterwards, the certificate must be sent to consensus
        // or Narwhal needs to shutdown, to avoid insistencies certificate store and
        // consensus dag.
        self.certificate_store.write(certificate.clone())?;

        // Update metrics for processed certificates.
        self.highest_processed_round = self.highest_processed_round.max(certificate.round());
        // TODO(metrics): Set `highest_processed_round` to `self.highest_processed_round as i64`
        // TODO(metrics): Increment `certificates_processed` by 1

        // Append the certificate to the aggregator of the
        // corresponding round.
        let digest = certificate.digest();
        if let Err(e) = self
            .append_certificate_in_aggregator(certificate.clone())
            .await
        {
            warn!(
                "Failed to aggregate certificate {} for header: {}",
                digest, e
            );
            return Err(DagError::ShuttingDown);
        }

        // Send it to the consensus layer.
        if let Err(e) = self.tx_new_certificates.send(certificate).await {
            warn!(
                "Failed to deliver certificate {} to the consensus: {}",
                digest, e
            );
            return Err(DagError::ShuttingDown);
        }

        Ok(())
    }

    async fn append_certificate_in_aggregator(
        &mut self,
        certificate: Certificate,
    ) -> DagResult<()> {
        // Check if we have enough certificates to enter a new dag round and propose a header.
        if let Some(parents) = self
            .certificates_aggregators
            .entry(certificate.round())
            .or_insert_with(|| Box::new(CertificatesAggregator::new()))
            .append(certificate.clone(), &self.committee)
        {
            // Send it to the `Proposer`.
            self.tx_parents
                .send((parents, certificate.round(), certificate.epoch()))
                .await
                .map_err(|_| DagError::ShuttingDown)?;
        }

        Ok(())
    }

    // Logs Core errors as appropriate.
    fn process_result(result: &DagResult<()>) {
        match result {
            Ok(()) => (),
            Err(DagError::StoreError(e)) => {
                error!("{e}");
                panic!("Storage failure: killing node.");
            }
            Err(
                e @ DagError::TooOld(..)
                | e @ DagError::VoteTooOld(..)
                | e @ DagError::InvalidEpoch { .. },
            ) => debug!("{e}"),
            Err(e) => warn!("{e}"),
        }
    }

    // Main loop listening to incoming messages.
    pub async fn run(mut self) -> DagResult<Self> {
        info!("Core on node {} has started successfully.", self.name);
        loop {
            let result = tokio::select! {
                Some((certificate, notify)) = self.rx_certificates.recv() => {
                    self.process_certificate(certificate, notify).await
                },

                // Here loopback certificates from the `CertificateFetcher` are received. These are
                // certificates fetched from other validators that are potentially missing locally.
                Some(message) = self.rx_certificates_loopback.recv() => {
                    let mut result = Ok(());
                    for cert in message.certificates {
                        result = match self.process_certificate(cert, None).await {
                            // It is possible that subsequent certificates are above GC round,
                            // so not stopping early.
                            Err(DagError::TooOld(_, _, _)) => continue,
                            result => result
                        };
                        if result.is_err() {
                            break;
                        }
                    };

                    if message.done.send(()).is_err() {
                        result = Err(DagError::ShuttingDown);
                    }

                    result
                },

                // We also receive here our new headers created by the `Proposer`.
                Some(header) = self.rx_headers.recv() => {
                    let (tx_cancel, rx_cancel) = oneshot::channel();
                    if let Some(cancel) = self.cancel_proposed_header {
                        let _ = cancel.send(());
                    }
                    self.cancel_proposed_header = Some(tx_cancel);

                    let name = self.name.clone();
                    let committee = self.committee.clone();
                    let header_store = self.header_store.clone();
                    let certificate_store = self.certificate_store.clone();
                    let signature_service = self.signature_service.clone();
                    let network = self.network.clone();
                    self.propose_header_tasks.spawn(Self::propose_header(
                        name,
                        committee,
                        header_store,
                        certificate_store,
                        signature_service,
                        network,
                        header,
                        rx_cancel,
                    ));
                    Ok(())
                },

                // Process certificates formed after receiving enough votes.
                Some(result) = self.propose_header_tasks.join_next() => {
                    match result {
                        Ok(Ok(certificate)) => {
                            self.process_own_certificate(certificate).await
                        },
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(DagError::ShuttingDown),
                    }
                },

                // Process any background task errors.
                Some(result) = self.background_tasks.join_next() => {
                    result.unwrap()  // propagate any panics
                },

                _ = self.rx_shutdown.receiver.recv() => {
                    return Ok(self);
                }

                // Check whether the consensus round has changed, to clean up structures
                Ok(()) = self.rx_consensus_round_updates.changed() => {
                    let round = *self.rx_consensus_round_updates.borrow();
                    if round > self.gc_depth {
                        let _now = Instant::now();

                        let gc_round = round - self.gc_depth;
                        self.certificates_aggregators.retain(|k, _| k > &gc_round);
                        self.gc_round = gc_round;

                        // TODO(metrics): Observe `now.elapsed().as_secs_f64()` as `gc_core_latency`
                    }

                    Ok(())
                }
            };

            Self::process_result(&result);
        }
    }
}
