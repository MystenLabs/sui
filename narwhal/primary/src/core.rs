// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    aggregators::{CertificatesAggregator, VotesAggregator},
    metrics::PrimaryMetrics,
    primary::PrimaryMessage,
    synchronizer::Synchronizer,
};
use async_recursion::async_recursion;
use config::{Committee, Epoch};
use crypto::{traits::VerifyingKey, Hash as _, SignatureService};
use network::{CancelHandler, MessageResult, PrimaryNetwork};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
use store::Store;
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};
use tracing::{debug, error, instrument, warn};
use types::{
    ensure,
    error::{DagError, DagResult},
    Certificate, CertificateDigest, Header, HeaderDigest, ReconfigureNotification, Round, Vote,
};

#[cfg(test)]
#[path = "tests/core_tests.rs"]
pub mod core_tests;

pub struct Core<PublicKey: VerifyingKey> {
    /// The public key of this primary.
    name: PublicKey,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// The persistent storage keyed to headers.
    header_store: Store<HeaderDigest, Header<PublicKey>>,
    /// The persistent storage keyed to certificates.
    certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
    /// Handles synchronization with other nodes and our workers.
    synchronizer: Synchronizer<PublicKey>,
    /// Service to sign headers.
    signature_service: SignatureService<PublicKey::Sig>,
    /// The current consensus round (used for cleanup).
    consensus_round: Arc<AtomicU64>,
    /// The depth of the garbage collector.
    gc_depth: Round,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Receiver for dag messages (headers, votes, certificates).
    rx_primaries: Receiver<PrimaryMessage<PublicKey>>,
    /// Receives loopback headers from the `HeaderWaiter`.
    rx_header_waiter: Receiver<Header<PublicKey>>,
    /// Receives loopback certificates from the `CertificateWaiter`.
    rx_certificate_waiter: Receiver<Certificate<PublicKey>>,
    /// Receives our newly created headers from the `Proposer`.
    rx_proposer: Receiver<Header<PublicKey>>,
    /// Output all certificates to the consensus layer.
    tx_consensus: Sender<Certificate<PublicKey>>,
    /// Send valid a quorum of certificates' ids to the `Proposer` (along with their round).
    tx_proposer: Sender<(Vec<Certificate<PublicKey>>, Round, Epoch)>,

    /// The last garbage collected round.
    gc_round: Round,
    /// The authors of the last voted headers.
    last_voted: HashMap<Round, HashSet<PublicKey>>,
    /// The set of headers we are currently processing.
    processing: HashMap<Round, HashSet<HeaderDigest>>,
    /// The last header we proposed (for which we are waiting votes).
    current_header: Header<PublicKey>,
    /// Aggregates votes into a certificate.
    votes_aggregator: VotesAggregator<PublicKey>,
    /// Aggregates certificates to use as parents for new headers.
    certificates_aggregators: HashMap<Round, Box<CertificatesAggregator<PublicKey>>>,
    /// A network sender to send the batches to the other workers.
    network: PrimaryNetwork,
    /// Keeps the cancel handlers of the messages we sent.
    cancel_handlers: HashMap<Round, Vec<CancelHandler<MessageResult>>>,
    /// Metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl<PublicKey: VerifyingKey> Core<PublicKey> {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        header_store: Store<HeaderDigest, Header<PublicKey>>,
        certificate_store: Store<CertificateDigest, Certificate<PublicKey>>,
        synchronizer: Synchronizer<PublicKey>,
        signature_service: SignatureService<PublicKey::Sig>,
        consensus_round: Arc<AtomicU64>,
        gc_depth: Round,
        rx_committee: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_primaries: Receiver<PrimaryMessage<PublicKey>>,
        rx_header_waiter: Receiver<Header<PublicKey>>,
        rx_certificate_waiter: Receiver<Certificate<PublicKey>>,
        rx_proposer: Receiver<Header<PublicKey>>,
        tx_consensus: Sender<Certificate<PublicKey>>,
        tx_proposer: Sender<(Vec<Certificate<PublicKey>>, Round, Epoch)>,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                name,
                committee,
                header_store,
                certificate_store,
                synchronizer,
                signature_service,
                consensus_round,
                gc_depth,
                rx_reconfigure: rx_committee,
                rx_primaries,
                rx_header_waiter,
                rx_certificate_waiter,
                rx_proposer,
                tx_consensus,
                tx_proposer,
                gc_round: 0,
                last_voted: HashMap::with_capacity(2 * gc_depth as usize),
                processing: HashMap::with_capacity(2 * gc_depth as usize),
                current_header: Header::default(),
                votes_aggregator: VotesAggregator::new(),
                certificates_aggregators: HashMap::with_capacity(2 * gc_depth as usize),
                network: Default::default(),
                cancel_handlers: HashMap::with_capacity(2 * gc_depth as usize),
                metrics,
            }
            .run()
            .await;
        })
    }

    #[instrument(level = "debug", skip_all)]
    async fn process_own_header(&mut self, header: Header<PublicKey>) -> DagResult<()> {
        if header.epoch < self.committee.epoch() {
            debug!("Proposer outdated");
            return Ok(());
        }

        // Update the committee now if the proposer already did so.
        self.try_update_committee();

        // Reset the votes aggregator.
        self.current_header = header.clone();
        self.votes_aggregator = VotesAggregator::new();

        // Broadcast the new header in a reliable manner.
        let addresses = self
            .committee
            .others_primaries(&self.name)
            .into_iter()
            .map(|(_, x)| x.primary_to_primary)
            .collect();

        let message = PrimaryMessage::Header(header.clone());
        let handlers = self.network.broadcast(addresses, &message).await;
        self.cancel_handlers
            .entry(header.round)
            .or_insert_with(Vec::new)
            .extend(handlers);

        // Process the header.
        self.process_header(&header).await
    }

    #[async_recursion]
    #[instrument(level = "debug", skip_all)]
    async fn process_header(&mut self, header: &Header<PublicKey>) -> DagResult<()> {
        debug!("Processing {:?} round:{:?}", header, header.round);
        let header_source = if self.name.eq(&header.author) {
            "own"
        } else {
            "other"
        };

        // Indicate that we are processing this header.
        let inserted = self
            .processing
            .entry(header.round)
            .or_insert_with(HashSet::new)
            .insert(header.id);

        if inserted {
            // Only increase the metric when the header has been seen for the first
            // time. Edge case is headers received past gc_round so we might have already
            // processed them, but not big issue for now.
            self.metrics
                .unique_headers_received
                .with_label_values(&[&header.epoch.to_string(), header_source])
                .inc();
        }

        // If the following condition is valid, it means we already garbage collected the parents. There is thus
        // no points in trying to synchronize them or vote for the header. We just need to gather the payload.
        if self.gc_round >= header.round {
            if self.synchronizer.missing_payload(header).await? {
                self.metrics
                    .headers_suspended
                    .with_label_values(&[&header.epoch.to_string(), "missing_payload"])
                    .inc();
                debug!("Downloading the payload of {header}");
            }
            return Ok(());
        }

        // Ensure we have the parents. If at least one parent is missing, the synchronizer returns an empty
        // vector; it will gather the missing parents (as well as all ancestors) from other nodes and then
        // reschedule processing of this header.
        let parents: Vec<Certificate<PublicKey>> = self.synchronizer.get_parents(header).await?;
        if parents.is_empty() {
            self.metrics
                .headers_suspended
                .with_label_values(&[&header.epoch.to_string(), "missing_parents"])
                .inc();
            debug!("Processing of {} suspended: missing parent(s)", header.id);
            return Ok(());
        }

        // Check the parent certificates. Ensure the parents form a quorum and are all from the previous round.
        let mut stake = 0;
        for x in parents {
            ensure!(
                x.round() + 1 == header.round,
                DagError::MalformedHeader(header.id)
            );
            stake += self.committee.stake(&x.origin());
        }
        ensure!(
            stake >= self.committee.quorum_threshold(),
            DagError::HeaderRequiresQuorum(header.id)
        );

        // Ensure we have the payload. If we don't, the synchronizer will ask our workers to get it, and then
        // reschedule processing of this header once we have it.
        if self.synchronizer.missing_payload(header).await? {
            self.metrics
                .headers_suspended
                .with_label_values(&[&header.epoch.to_string(), "missing_payload"])
                .inc();
            debug!("Processing of {header} suspended: missing payload");
            return Ok(());
        }

        // Store the header.
        self.header_store.write(header.id, header.clone()).await;

        self.metrics
            .headers_processed
            .with_label_values(&[&header.epoch.to_string(), header_source])
            .inc();

        // Check if we can vote for this header.
        // TODO: compute from a persisted last_voted (issue #488)
        if self
            .last_voted
            .entry(header.round)
            .or_insert_with(HashSet::new)
            .insert(header.author.clone())
        {
            // Make a vote and send it to the header's creator.
            let vote = Vote::new(header, &self.name, &mut self.signature_service).await;
            debug!(
                "Created vote {vote:?} for {header} at round {}",
                header.round
            );
            if vote.origin == self.name {
                // This used to be a panic, but this may fail if we're processing a late-received
                // header where we were already are part of the signers (as a restart could have blown our `last_voted`)
                // TODO: change this back to a panic-on-error once we have persisted last_voted (issue #488)
                let res = self.process_vote(vote).await;
                if let Err(e) = res {
                    error!("Failed to process our own vote: {}", e.to_string());
                }
            } else {
                let address = self
                    .committee
                    .primary(&header.author)
                    .expect("Author of valid header is not in the committee")
                    .primary_to_primary;
                let handler = self
                    .network
                    .send(address, &PrimaryMessage::Vote(vote))
                    .await;
                self.cancel_handlers
                    .entry(header.round)
                    .or_insert_with(Vec::new)
                    .push(handler);
            }
        }
        Ok(())
    }

    #[async_recursion]
    #[instrument(level = "debug", skip_all)]
    async fn process_vote(&mut self, vote: Vote<PublicKey>) -> DagResult<()> {
        debug!("Processing {:?}", vote);

        // Add it to the votes' aggregator and try to make a new certificate.
        if let Some(certificate) =
            self.votes_aggregator
                .append(vote, &self.committee, &self.current_header)?
        {
            debug!("Assembled {:?}", certificate);

            // Broadcast the certificate.
            let addresses = self
                .committee
                .others_primaries(&self.name)
                .into_iter()
                .map(|(_, x)| x.primary_to_primary)
                .collect();
            let message = PrimaryMessage::Certificate(certificate.clone());
            let handlers = self.network.broadcast(addresses, &message).await;
            self.cancel_handlers
                .entry(certificate.round())
                .or_insert_with(Vec::new)
                .extend(handlers);

            self.metrics
                .certificates_created
                .with_label_values(&[&certificate.epoch().to_string()])
                .inc();

            // Process the new certificate.
            match self.process_certificate(certificate).await {
                Ok(()) => (),
                result @ Err(DagError::ShuttingDown) => result?,
                _ => panic!("Failed to process valid certificate"),
            }
        }
        Ok(())
    }

    #[async_recursion]
    #[instrument(level = "debug", skip_all)]
    async fn process_certificate(&mut self, certificate: Certificate<PublicKey>) -> DagResult<()> {
        debug!(
            "Processing {:?} round:{:?}",
            certificate,
            certificate.round()
        );

        // Let the proposer draw early conclusions from a certificate at this round and epoch, without its
        // parents or payload (which we may not have yet).
        //
        // Since our certificate is well-signed, it shows a majority of honest signers stand at round r,
        // so to make a successful proposal, our proposer must use parents at least at round r-1.
        //
        // This allows the proposer not to fire proposals at rounds strictly below the certificate we witnessed.
        let minimal_round_for_parents = certificate.round().saturating_sub(1);
        self.tx_proposer
            .send((vec![], minimal_round_for_parents, certificate.epoch()))
            .await
            .map_err(|_| DagError::ShuttingDown)?;

        // Process the header embedded in the certificate if we haven't already voted for it (if we already
        // voted, it means we already processed it). Since this header got certified, we are sure that all
        // the data it refers to (ie. its payload and its parents) are available. We can thus continue the
        // processing of the certificate even if we don't have them in store right now.
        if !self
            .processing
            .get(&certificate.header.round)
            .map_or_else(|| false, |x| x.contains(&certificate.header.id))
        {
            // This function may still throw an error if the storage fails.
            self.process_header(&certificate.header).await?;
        }

        // Ensure we have all the ancestors of this certificate yet (if we didn't already garbage collect them).
        // If we don't, the synchronizer will gather them and trigger re-processing of this certificate.
        if certificate.round() > self.gc_round + 1
            && !self.synchronizer.deliver_certificate(&certificate).await?
        {
            debug!(
                "Processing of {:?} suspended: missing ancestors",
                certificate
            );
            self.metrics
                .certificates_suspended
                .with_label_values(&[&certificate.epoch().to_string(), "missing_parents"])
                .inc();
            return Ok(());
        }

        // Store the certificate.
        self.certificate_store
            .write(certificate.digest(), certificate.clone())
            .await;

        let certificate_source = if self.name.eq(&certificate.header.author) {
            "own"
        } else {
            "other"
        };
        self.metrics
            .certificates_processed
            .with_label_values(&[&certificate.epoch().to_string(), certificate_source])
            .inc();

        // Check if we have enough certificates to enter a new dag round and propose a header.
        if let Some(parents) = self
            .certificates_aggregators
            .entry(certificate.round())
            .or_insert_with(|| Box::new(CertificatesAggregator::new()))
            .append(certificate.clone(), &self.committee)
        {
            // Send it to the `Proposer`.
            self.tx_proposer
                .send((parents, certificate.round(), certificate.epoch()))
                .await
                .map_err(|_| DagError::ShuttingDown)?;
        }

        // Send it to the consensus layer.
        let id = certificate.header.id;
        if let Err(e) = self.tx_consensus.send(certificate).await {
            warn!(
                "Failed to deliver certificate {} to the consensus: {}",
                id, e
            );
        }
        Ok(())
    }

    fn sanitize_header(&mut self, header: &Header<PublicKey>) -> DagResult<()> {
        if header.epoch > self.committee.epoch() {
            self.try_update_committee();
        }
        ensure!(
            self.committee.epoch() == header.epoch,
            DagError::InvalidEpoch {
                expected: self.committee.epoch(),
                received: header.epoch
            }
        );
        ensure!(
            self.gc_round < header.round,
            DagError::TooOld(header.id.into(), header.round)
        );

        // Verify the header's signature.
        header.verify(&self.committee)?;

        // TODO [issue #3]: Prevent bad nodes from sending junk headers with high round numbers.

        Ok(())
    }

    fn sanitize_vote(&mut self, vote: &Vote<PublicKey>) -> DagResult<()> {
        if vote.epoch > self.committee.epoch() {
            self.try_update_committee();
        }
        ensure!(
            self.committee.epoch() == vote.epoch,
            DagError::InvalidEpoch {
                expected: self.committee.epoch(),
                received: vote.epoch
            }
        );
        ensure!(
            self.current_header.round <= vote.round,
            DagError::TooOld(vote.digest().into(), vote.round)
        );

        // Ensure we receive a vote on the expected header.
        ensure!(
            vote.id == self.current_header.id
                && vote.origin == self.current_header.author
                && vote.round == self.current_header.round,
            DagError::UnexpectedVote(vote.id)
        );

        // Verify the vote.
        vote.verify(&self.committee).map_err(DagError::from)
    }

    fn sanitize_certificate(&mut self, certificate: &Certificate<PublicKey>) -> DagResult<()> {
        if certificate.epoch() > self.committee.epoch() {
            self.try_update_committee();
        }
        ensure!(
            self.committee.epoch() == certificate.epoch(),
            DagError::InvalidEpoch {
                expected: self.committee.epoch(),
                received: certificate.epoch()
            }
        );
        ensure!(
            self.gc_round < certificate.round(),
            DagError::TooOld(certificate.digest().into(), certificate.round())
        );

        // Verify the certificate (and the embedded header).
        certificate.verify(&self.committee).map_err(DagError::from)
    }

    /// If a new committee is available, update our internal state.
    fn try_update_committee(&mut self) {
        if self
            .rx_reconfigure
            .has_changed()
            .expect("Reconfigure channel dropped")
        {
            let message = self.rx_reconfigure.borrow().clone();
            if let ReconfigureNotification::NewCommittee(new_committee) = message {
                self.update_committee(new_committee);
                // Mark the value as seen.
                let _ = self.rx_reconfigure.borrow_and_update();
            }
        }
    }

    /// Update the committee and cleanup internal state.
    fn update_committee(&mut self, committee: Committee<PublicKey>) {
        tracing::info!("Committee updated to epoch {}", committee.epoch);
        self.last_voted.clear();
        self.processing.clear();
        self.certificates_aggregators.clear();
        self.cancel_handlers.clear();

        self.committee = committee;
        self.synchronizer.update_genesis(&self.committee);
        tracing::debug!("Committee updated to {}", self.committee);
    }

    // Main loop listening to incoming messages.
    pub async fn run(&mut self) {
        loop {
            let result = tokio::select! {
                // We receive here messages from other primaries.
                Some(message) = self.rx_primaries.recv() => {
                    match message {
                        PrimaryMessage::Header(header) => {
                            match self.sanitize_header(&header) {
                                Ok(()) => self.process_header(&header).await,
                                error => error
                            }

                        },
                        PrimaryMessage::Vote(vote) => {
                            match self.sanitize_vote(&vote) {
                                Ok(()) => self.process_vote(vote).await,
                                error => error
                            }
                        },
                        PrimaryMessage::Certificate(certificate) => {
                            match self.sanitize_certificate(&certificate) {
                                Ok(()) =>  self.process_certificate(certificate).await,
                                error => error
                            }
                        },
                        _ => panic!("Unexpected core message")
                    }
                },

                // We receive here loopback headers from the `HeaderWaiter`. Those are headers for which we interrupted
                // execution (we were missing some of their dependencies) and we are now ready to resume processing.
                Some(header) = self.rx_header_waiter.recv() => self.process_header(&header).await,

                // We receive here loopback certificates from the `CertificateWaiter`. Those are certificates for which
                // we interrupted execution (we were missing some of their ancestors) and we are now ready to resume
                // processing.
                Some(certificate) = self.rx_certificate_waiter.recv() => self.process_certificate(certificate).await,

                // We also receive here our new headers created by the `Proposer`.
                Some(header) = self.rx_proposer.recv() => self.process_own_header(header).await,

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            self.update_committee(new_committee);
                            Ok(())
                        },
                        ReconfigureNotification::Shutdown => return
                    }
                }
            };
            match result {
                Ok(()) => (),
                Err(e @ DagError::ShuttingDown) => debug!("{e}"),
                Err(DagError::StoreError(e)) => {
                    error!("{e}");
                    panic!("Storage failure: killing node.");
                }
                Err(e @ DagError::TooOld(..) | e @ DagError::InvalidEpoch { .. }) => debug!("{e}"),
                Err(e) => warn!("{e}"),
            }

            // Cleanup internal state.
            let round = self.consensus_round.load(Ordering::Relaxed);
            if round > self.gc_depth {
                let now = Instant::now();

                let gc_round = round - self.gc_depth;
                self.last_voted.retain(|k, _| k > &gc_round);
                self.processing.retain(|k, _| k > &gc_round);
                self.certificates_aggregators.retain(|k, _| k > &gc_round);
                self.cancel_handlers.retain(|k, _| k > &gc_round);
                self.gc_round = gc_round;

                self.metrics
                    .gc_core_latency
                    .with_label_values(&[&self.committee.epoch.to_string()])
                    .observe(now.elapsed().as_secs_f64());
            }

            self.metrics
                .core_cancel_handlers_total
                .with_label_values(&[&self.committee.epoch.to_string()])
                .set(self.cancel_handlers.len() as i64);
        }
    }
}
