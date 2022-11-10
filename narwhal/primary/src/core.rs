// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    aggregators::{CertificatesAggregator, VotesAggregator},
    certificate_waiter::CertificateLoopbackMessage,
    metrics::PrimaryMetrics,
    primary::PrimaryMessage,
    synchronizer::Synchronizer,
};

use config::{Committee, Epoch, SharedWorkerCache};
use crypto::{NetworkPublicKey, PublicKey, Signature};
use fastcrypto::{hash::Hash as _, SignatureService};
use futures::StreamExt;
use futures::{future::OptionFuture, stream::FuturesUnordered};
use network::{anemo_ext::NetworkExt, P2pNetwork, UnreliableNetwork};
use std::time::Duration;
use std::{collections::HashMap, sync::Arc, time::Instant};
use storage::CertificateStore;
use store::Store;
use sui_metrics::spawn_monitored_task;
use tokio::{
    sync::{oneshot, watch},
    task::{JoinHandle, JoinSet},
};
use tracing::{debug, error, info, instrument, trace, warn};
use types::{
    ensure,
    error::{DagError, DagResult},
    metered_channel::{Receiver, Sender},
    Certificate, CertificateDigest, Header, HeaderDigest, PrimaryToPrimaryClient,
    ReconfigureNotification, RequestVoteRequest, Round, Timestamp, Vote,
};

#[cfg(test)]
#[path = "tests/core_tests.rs"]
pub mod core_tests;

pub struct Core {
    /// The public key of this primary.
    name: PublicKey,
    /// The committee information.
    committee: Committee,
    /// The worker information cache.
    worker_cache: SharedWorkerCache,
    /// The persistent storage keyed to headers.
    header_store: Store<HeaderDigest, Header>,
    /// The persistent storage keyed to certificates.
    certificate_store: CertificateStore,
    /// Handles synchronization with other nodes and our workers.
    synchronizer: Arc<Synchronizer>,
    /// Service to sign headers.
    signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
    /// Get a signal when the round changes
    rx_consensus_round_updates: watch::Receiver<Round>,
    /// The depth of the garbage collector.
    gc_depth: Round,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Receiver for certificates.
    rx_certificates: Receiver<(Certificate, Option<oneshot::Sender<DagResult<()>>>)>,
    /// Receives loopback certificates from the `CertificateWaiter`.
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
    /// Contains tasks that are synchronizing worker batches for processed certificates.
    batch_sync_tasks: JoinSet<DagResult<()>>,
    /// Used to cancel vote requests for a previously-proposed header that is being replaced
    /// before a certificate could be formed.
    cancel_proposed_header: Option<oneshot::Sender<()>>,
    /// Handle to propose_header task.
    propose_header_future: OptionFuture<JoinHandle<DagResult<Certificate>>>,
    /// Aggregates certificates to use as parents for new headers.
    certificates_aggregators: HashMap<Round, Box<CertificatesAggregator>>,
    /// A network sender to send the batches to the other workers.
    network: P2pNetwork,
    /// Metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        worker_cache: SharedWorkerCache,
        header_store: Store<HeaderDigest, Header>,
        certificate_store: CertificateStore,
        synchronizer: Arc<Synchronizer>,
        signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
        rx_consensus_round_updates: watch::Receiver<u64>,
        gc_depth: Round,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_certificates: Receiver<(Certificate, Option<oneshot::Sender<DagResult<()>>>)>,
        rx_certificates_loopback: Receiver<CertificateLoopbackMessage>,
        rx_headers: Receiver<Header>,
        tx_new_certificates: Sender<Certificate>,
        tx_parents: Sender<(Vec<Certificate>, Round, Epoch)>,
        metrics: Arc<PrimaryMetrics>,
        primary_network: P2pNetwork,
    ) -> JoinHandle<()> {
        spawn_monitored_task!(async move {
            Self {
                name,
                committee,
                worker_cache,
                header_store,
                certificate_store,
                synchronizer,
                signature_service,
                rx_consensus_round_updates,
                gc_depth,
                rx_reconfigure,
                rx_certificates,
                rx_certificates_loopback,
                rx_headers,
                tx_new_certificates,
                tx_parents,
                gc_round: 0,
                highest_received_round: 0,
                highest_processed_round: 0,
                pending_certificates: HashMap::new(),
                batch_sync_tasks: JoinSet::new(),
                cancel_proposed_header: None,
                propose_header_future: None.into(),
                certificates_aggregators: HashMap::with_capacity(2 * gc_depth as usize),
                network: primary_network,
                metrics,
            }
            .recover()
            .await
            .run()
            .await;
        })
    }

    #[instrument(level = "info", skip_all)]
    pub async fn recover(mut self) -> Self {
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
            self.append_certificate_in_aggregator(certificate)
                .await
                .expect("Failed appending recovered certificates to aggregator in primary core");
        }

        self.highest_received_round = last_round_number;
        self.highest_processed_round = last_round_number;

        self
    }

    // Requests a vote for a Header from the given peer. Retries indefinitely until either a
    // vote is received, or a permanent error is returned.
    #[instrument(level = "debug", skip_all, fields(header_digest = ?header.digest()))]
    async fn request_vote(
        network: anemo::Network,
        committee: Committee,
        certificate_store: CertificateStore,
        target: NetworkPublicKey,
        header: Header,
    ) -> DagResult<Vote> {
        let peer_id = anemo::PeerId(target.0.to_bytes());
        let peer = network.waiting_peer(peer_id);
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
                    .read_all(missing_parents.into_iter())?
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
                && vote.round == header.round,
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
        metrics: Arc<PrimaryMetrics>,
        network: anemo::Network,
        header: Header,
        mut cancel: oneshot::Receiver<()>,
    ) -> DagResult<Certificate> {
        if header.epoch < committee.epoch() {
            debug!("Proposer outdated");
            return Err(DagError::InvalidEpoch {
                expected: committee.epoch(),
                received: header.epoch,
            });
        }

        // Process the header.
        // DO NOT MERGE: Any need to verify or protect against equivocation from self-produced
        // headers? I'm thinking not but wanted to verify.
        metrics
            .headers_proposed
            .with_label_values(&[&header.epoch.to_string()])
            .inc();
        header_store
            .async_write(header.digest(), header.clone())
            .await;

        // Reset the votes aggregator and sign our own header.
        let mut votes_aggregator = VotesAggregator::new();
        let vote = Vote::new(&header, &name, &signature_service).await;
        let mut certificate = votes_aggregator.append(vote, &committee, &header)?;

        // Trigger vote requests.
        let peers = committee
            .others_primaries(&name)
            .into_iter()
            .map(|(_, _, network_key)| network_key);
        let mut requests: FuturesUnordered<_> = peers
            .map(|peer| {
                let header = header.clone();
                Self::request_vote(
                    network.clone(),
                    committee.clone(),
                    certificate_store.clone(),
                    peer,
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
                        Some(Err(e)) => debug!("failed to get vote for header {header}: {e:?}"),
                        None => break,
                    }
                },
                _ = &mut cancel => {
                    debug!("canceling Header proposal {header} for round {}", header.round);
                    return Err(DagError::Canceled)
                },
            }
        }

        // Check if we successfully formed a certificate.
        let certificate =
            certificate.ok_or_else(|| DagError::CouldNotFormCertificate(header.digest()))?;
        debug!("Assembled {certificate:?}");
        Ok(certificate)
    }

    #[instrument(level = "debug", skip_all, fields(certificate_digest = ?certificate.digest()))]
    async fn process_own_certificate(&mut self, certificate: Certificate) -> DagResult<()> {
        self.metrics
            .certificates_created
            .with_label_values(&[&certificate.epoch().to_string()])
            .inc();
        self.metrics
            .header_to_certificate_latency
            .with_label_values(&[&certificate.epoch().to_string()])
            .observe(
                certificate
                    .header
                    .metadata
                    .created_at
                    .elapsed()
                    .as_secs_f64(),
            );

        // Broadcast the certificate.
        let network_keys = self
            .committee
            .others_primaries(&self.name)
            .into_iter()
            .map(|(_, _, network_key)| network_key)
            .collect();
        self.network.unreliable_broadcast(
            network_keys,
            &PrimaryMessage::Certificate(certificate.clone()),
        );

        // Process the new certificate.
        match self.process_certificate_internal(certificate).await {
            Ok(()) => Ok(()),
            result @ Err(DagError::ShuttingDown) => result,
            _ => panic!("Failed to process locally-created certificate"),
        }
    }

    #[instrument(level = "debug", skip_all, fields(certificate_digest = ?certificate.digest()))]
    async fn process_certificate(
        &mut self,
        certificate: Certificate,
        notify: Option<oneshot::Sender<DagResult<()>>>,
    ) -> DagResult<()> {
        let digest = certificate.digest();
        match self.process_certificate_internal(certificate).await {
            Ok(()) => {
                if let Some(notify) = notify {
                    let _ = notify.send(Ok(())); // no problem if remote side isn't listening
                }
                if let Some(notifies) = self.pending_certificates.remove(&digest) {
                    for notify in notifies {
                        let _ = notify.send(Ok(())); // no problem if remote side isn't listening
                    }
                }
                Ok(())
            }
            Err(DagError::Suspended) => {
                if let Some(notify) = notify {
                    self.pending_certificates
                        .entry(digest)
                        .or_insert_with(Vec::new)
                        .push(notify);
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    // #[async_recursion]
    #[instrument(level = "debug", skip_all, fields(certificate_digest = ?certificate.digest()))]
    async fn process_certificate_internal(&mut self, certificate: Certificate) -> DagResult<()> {
        if self.certificate_store.read(certificate.digest())?.is_some() {
            trace!(
                "Certificate {} has already been processed. Skip processing.",
                certificate.digest()
            );
            return Ok(());
        }

        debug!(
            "Processing certificate {:?} round:{:?}",
            certificate,
            certificate.round()
        );

        let certificate_source = if self.name.eq(&certificate.header.author) {
            "own"
        } else {
            "other"
        };
        self.highest_received_round = self.highest_received_round.max(certificate.round());
        self.metrics
            .highest_received_round
            .with_label_values(&[&certificate.epoch().to_string(), certificate_source])
            .set(self.highest_received_round as i64);

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
        let network = self.network.network();
        let max_age = self.gc_depth.saturating_sub(1);
        self.batch_sync_tasks
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
            self.metrics
                .certificates_suspended
                .with_label_values(&[&certificate.epoch().to_string(), "missing_parents"])
                .inc();
            return Err(DagError::Suspended);
        }

        // Store the certificate.
        self.certificate_store.write(certificate.clone())?;

        // Update metrics for processed certificates.
        self.highest_processed_round = self.highest_processed_round.max(certificate.round());
        self.metrics
            .highest_processed_round
            .with_label_values(&[&certificate.epoch().to_string(), certificate_source])
            .set(self.highest_processed_round as i64);
        self.metrics
            .certificates_processed
            .with_label_values(&[&certificate.epoch().to_string(), certificate_source])
            .inc();
        // Append the certificate to the aggregator of the
        // corresponding round.
        self.append_certificate_in_aggregator(certificate.clone())
            .await?;

        // Send it to the consensus layer.
        let digest = certificate.header.digest();
        if let Err(e) = self.tx_new_certificates.send(certificate).await {
            warn!(
                "Failed to deliver certificate {} to the consensus: {}",
                digest, e
            );
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

    async fn sanitize_certificate(&mut self, certificate: &Certificate) -> DagResult<()> {
        if certificate.epoch() > self.committee.epoch() {
            self.try_update_committee().await;
        }
        ensure!(
            self.committee.epoch() == certificate.epoch(),
            DagError::InvalidEpoch {
                expected: self.committee.epoch(),
                received: certificate.epoch()
            }
        );
        // Ok to drop old certificate, because it will never be included into the consensus dag.
        ensure!(
            self.gc_round < certificate.round(),
            DagError::TooOld(
                certificate.digest().into(),
                certificate.round(),
                self.gc_round
            )
        );
        // Verify the certificate (and the embedded header).
        certificate
            .verify(&self.committee, self.worker_cache.clone())
            .map_err(DagError::from)
    }

    /// If a new committee is available, update our internal state.
    async fn try_update_committee(&mut self) {
        if self
            .rx_reconfigure
            .has_changed()
            .expect("Reconfigure channel dropped")
        {
            let message = self.rx_reconfigure.borrow().clone();
            if let ReconfigureNotification::NewEpoch(new_committee) = message {
                self.change_epoch(new_committee).await;
                // Mark the value as seen.
                let _ = self.rx_reconfigure.borrow_and_update();
            }
        }
    }

    /// Update the committee and cleanup internal state.
    async fn change_epoch(&mut self, committee: Committee) {
        self.certificates_aggregators.clear();
        self.committee = committee;
    }

    // Logs Core errors as appropriate.
    fn process_result(result: &DagResult<()>) {
        match result {
            Ok(()) => (),
            Err(e @ DagError::ShuttingDown) => debug!("{e}"),
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
    pub async fn run(mut self) {
        info!("Core on node {} has started successfully.", self.name);
        loop {
            let result = tokio::select! {
                Some((certificate, notify)) = self.rx_certificates.recv() => {
                    match self.sanitize_certificate(&certificate).await {
                        Ok(()) =>  self.process_certificate(certificate, notify).await,
                        error => {
                            // `error` is consumed by the notify, so we process it first manually
                            // and then just return Ok.
                            Self::process_result(&error);
                            if let Some(notify) = notify {
                                let _ = notify.send(error);
                            }
                            Ok(())
                        },
                    }
                },

                // Here loopback certificates from the `CertificateWaiter` are received. These are
                // certificates fetched from other validators that are potentially missing locally.
                Some(message) = self.rx_certificates_loopback.recv() => {
                    let mut result = Ok(());
                    for cert in message.certificates {
                        result = match self.sanitize_certificate(&cert).await {
                            // TODO: consider moving some checks to CertificateWaiter, and skipping
                            // those checks here?
                            Ok(()) => self.process_certificate(cert, None).await,
                            // It is possible that subsequent certificates are above GC round,
                            // so not stopping early.
                            Err(DagError::TooOld(_, _, _)) => continue,
                            error => error
                        };
                        if result.is_err() {
                            break;
                        }
                    };
                    message.done.send(()).expect("Failed to signal back to CertificateWaiter");
                    result
                },

                // We also receive here our new headers created by the `Proposer`.
                Some(header) = self.rx_headers.recv() => {
                    let (tx_cancel, rx_cancel) = oneshot::channel();
                    if self.cancel_proposed_header.is_some() {
                        let cancel = std::mem::replace(&mut self.cancel_proposed_header, Some(tx_cancel));
                        let _ = cancel.unwrap().send(());
                    } else {
                        self.cancel_proposed_header = Some(tx_cancel);
                    }

                    self.try_update_committee().await;

                    let name = self.name.clone();
                    let committee = self.committee.clone();
                    let header_store = self.header_store.clone();
                    let certificate_store = self.certificate_store.clone();
                    let signature_service = self.signature_service.clone();
                    let metrics = self.metrics.clone();
                    let network = self.network.network();
                    self.propose_header_future = Some(spawn_monitored_task!(Self::propose_header(
                        name,
                        committee,
                        header_store,
                        certificate_store,
                        signature_service,
                        metrics,
                        network,
                        header,
                        rx_cancel,
                    ))).into();
                    Ok(())
                },

                // Process certificates formed after receiving enough votes.
                Some(result) = &mut self.propose_header_future => {
                    // Clear the future so we only process it once.
                    self.propose_header_future = None.into();

                    match result {
                        Ok(Ok(certificate)) => {
                            self.process_own_certificate(certificate).await
                        },
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(DagError::ShuttingDown),
                    }
                },

                // Process any errors syncing batches.
                Some(result) = self.batch_sync_tasks.join_next() => {
                    result.unwrap()
                },

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(new_committee) => {
                            self.change_epoch(new_committee).await;
                        },
                        ReconfigureNotification::UpdateCommittee(new_committee) => {
                            // Update the committee.
                            self.committee = new_committee;
                        },
                        ReconfigureNotification::Shutdown => {
                            if self.cancel_proposed_header.is_some() {
                                let cancel = std::mem::replace(
                                    &mut self.cancel_proposed_header,
                                    None
                                );
                                let _ = cancel.unwrap().send(());
                            }
                            return
                        }
                    }
                    tracing::debug!("Committee updated to {}", self.committee);
                    Ok(())
                },

                // Check whether the consensus round has changed, to clean up structures
                Ok(()) = self.rx_consensus_round_updates.changed() => {
                    let round = *self.rx_consensus_round_updates.borrow();
                    if round > self.gc_depth {
                        let now = Instant::now();

                        let gc_round = round - self.gc_depth;
                        self.certificates_aggregators.retain(|k, _| k > &gc_round);
                        self.gc_round = gc_round;

                        self.metrics
                            .gc_core_latency
                            .with_label_values(&[&self.committee.epoch.to_string()])
                            .observe(now.elapsed().as_secs_f64());
                    }

                    Ok(())
                }
            };

            Self::process_result(&result);
        }
    }
}
