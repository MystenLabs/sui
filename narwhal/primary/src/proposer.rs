// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{metrics::PrimaryMetrics, NetworkModel};
use config::{Committee, Epoch, WorkerId};
use crypto::{PublicKey, Signature};
use fastcrypto::{hash::Hash as _, SignatureService};
use std::{cmp::Ordering, sync::Arc};
use storage::ProposerStore;
use tokio::time::Instant;
use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{sleep, Duration},
};
use tracing::{debug, info};
use types::{
    error::{DagError, DagResult},
    metered_channel::{Receiver, Sender},
    BatchDigest, Certificate, Header, ReconfigureNotification, Round, Timestamp, TimestampMs,
};

#[cfg(test)]
#[path = "tests/proposer_tests.rs"]
pub mod proposer_tests;

/// The proposer creates new headers and send them to the core for broadcasting and further processing.
pub struct Proposer {
    /// The public key of this primary.
    name: PublicKey,
    /// The committee information.
    committee: Committee,
    /// Service to sign headers.
    signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
    /// The threshold number of batches that can trigger
    /// a header creation. When there are available at least
    /// `header_num_of_batches_threshold` batches we are ok
    /// to try and propose a header
    header_num_of_batches_threshold: usize,
    /// The maximum number of batches in header.
    max_header_num_of_batches: usize,
    /// The maximum delay to wait for batches' digests.
    max_header_delay: Duration,
    /// The network model in which the node operates.
    network_model: NetworkModel,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    /// Receives the parents to include in the next header (along with their round number) from core.
    rx_parents: Receiver<(Vec<Certificate>, Round, Epoch)>,
    /// Receives the batches' digests from our workers.
    rx_our_digests: Receiver<(BatchDigest, WorkerId, TimestampMs)>,
    /// Sends newly created headers to the `Core`.
    tx_headers: Sender<Header>,

    /// The proposer store for persisting the last header.
    proposer_store: ProposerStore,
    /// The current round of the dag.
    round: Round,
    /// Holds the certificates' ids waiting to be included in the next header.
    last_parents: Vec<Certificate>,
    /// Holds the certificate of the last leader (if any).
    last_leader: Option<Certificate>,
    /// Holds the batches' digests waiting to be included in the next header.
    digests: Vec<(BatchDigest, WorkerId, TimestampMs)>,
    /// Metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl Proposer {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn spawn(
        name: PublicKey,
        committee: Committee,
        signature_service: SignatureService<Signature, { crypto::DIGEST_LENGTH }>,
        proposer_store: ProposerStore,
        header_num_of_batches_threshold: usize,
        max_header_num_of_batches: usize,
        max_header_delay: Duration,
        network_model: NetworkModel,
        rx_reconfigure: watch::Receiver<ReconfigureNotification>,
        rx_parents: Receiver<(Vec<Certificate>, Round, Epoch)>,
        rx_our_digests: Receiver<(BatchDigest, WorkerId, TimestampMs)>,
        tx_headers: Sender<Header>,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        let genesis = Certificate::genesis(&committee);
        tokio::spawn(async move {
            Self {
                name,
                committee,
                signature_service,
                header_num_of_batches_threshold,
                max_header_num_of_batches,
                max_header_delay,
                network_model,
                rx_reconfigure,
                rx_parents,
                rx_our_digests,
                tx_headers,
                proposer_store,
                round: 0,
                last_parents: genesis,
                last_leader: None,
                digests: Vec::with_capacity(2 * max_header_num_of_batches),
                metrics,
            }
            .run()
            .await;
        })
    }

    /// make_header creates a new Header, persists it to database
    /// and sends it to core for processing. If successful, it returns
    /// the number of batch digests included in header.
    async fn make_header(&mut self) -> DagResult<usize> {
        // Make a new header.
        let header = self.create_new_header().await?;

        // Store the last header.
        self.proposer_store.write_last_proposed(&header)?;

        #[cfg(feature = "benchmark")]
        for digest in header.payload.keys() {
            // NOTE: This log entry is used to compute performance.
            info!("Created {} -> {:?}", header, digest);
        }

        let num_of_included_digests = header.payload.len();

        // Send the new header to the `Core` that will broadcast and process it.
        self.tx_headers
            .send(header)
            .await
            .map_err(|_| DagError::ShuttingDown)?;

        Ok(num_of_included_digests)
    }

    // Creates a new header. Also the method ensures we are protected against equivocation.
    // If we detect that a different header has been already produced for the same round, then
    // this method returns the earlier header. Otherwise the newly created header will be returned.
    async fn create_new_header(&mut self) -> DagResult<Header> {
        // Make a new header.
        let num_of_digests = self.digests.len().min(self.max_header_num_of_batches);
        let digests: Vec<_> = self.digests.drain(..num_of_digests).collect();

        let header = Header::new(
            self.name.clone(),
            self.round,
            self.committee.epoch(),
            digests
                .iter()
                .map(|(digest, worker_id, _)| (*digest, *worker_id))
                .collect(),
            self.last_parents.drain(..).map(|x| x.digest()).collect(),
            &mut self.signature_service,
        )
        .await;

        if let Some(last_header) = self.proposer_store.get_last_proposed()? {
            if last_header.round == header.round && last_header.epoch == header.epoch {
                // We have already produced a header for the current round, idempotent re-send
                if last_header != header {
                    debug!("Equivocation protection was enacted in the proposer");

                    // add again the digests for proposal
                    // keeping their original order
                    digests
                        .iter()
                        .for_each(|value| self.digests.insert(0, *value));

                    return Ok(last_header);
                }
            }
        }

        for (_, _, created_at_timestamp) in digests {
            self.metrics
                .proposer_batch_latency
                .observe(created_at_timestamp.elapsed().as_secs_f64());
        }

        Ok(header)
    }

    /// Update the committee and cleanup internal state.
    fn change_epoch(&mut self, committee: Committee) {
        self.committee = committee;

        self.round = 0;
        self.last_parents = Certificate::genesis(&self.committee);
    }

    /// Compute the timeout value of the proposer.
    fn timeout_value(&self) -> Instant {
        match self.network_model {
            // In partial synchrony, if this node is going to be the leader of the next
            // round, we set a lower timeout value to increase its chance of committing
            // the leader committed.
            NetworkModel::PartiallySynchronous
                if self.committee.leader(self.round + 1) == self.name =>
            {
                Instant::now() + self.max_header_delay / 2
            }

            // Otherwise we keep the default timeout value.
            _ => Instant::now() + self.max_header_delay,
        }
    }

    /// Update the last leader certificate. This is only relevant in partial synchrony.
    fn update_leader(&mut self) -> bool {
        let leader_name = self.committee.leader(self.round);
        self.last_leader = self
            .last_parents
            .iter()
            .find(|x| x.origin() == leader_name)
            .cloned();

        if let Some(leader) = self.last_leader.as_ref() {
            debug!("Got leader {} for round {}", leader.origin(), self.round);
        }

        self.last_leader.is_some()
    }

    /// Check whether if we have (i) f+1 votes for the leader, (ii) 2f+1 nodes not voting for the leader,
    /// or (iii) there is no leader to vote for. This is only relevant in partial synchrony.
    fn enough_votes(&self) -> bool {
        let leader = match &self.last_leader {
            Some(x) => x.digest(),
            None => return true,
        };

        let mut votes_for_leader = 0;
        let mut no_votes = 0;
        for certificate in &self.last_parents {
            let stake = self.committee.stake(&certificate.origin());
            if certificate.header.parents.contains(&leader) {
                votes_for_leader += stake;
            } else {
                no_votes += stake;
            }
        }

        let mut enough_votes = votes_for_leader >= self.committee.validity_threshold();
        if enough_votes {
            if let Some(leader) = self.last_leader.as_ref() {
                debug!(
                    "Got enough support for leader {} at round {}",
                    leader.origin(),
                    self.round
                );
            }
        }
        enough_votes |= no_votes >= self.committee.quorum_threshold();
        enough_votes
    }

    /// Whether we can advance the DAG or need to wait for the leader/more votes. This is only relevant in
    /// partial synchrony. Note that if we timeout, we ignore this check and advance anyway.
    fn ready(&mut self) -> bool {
        match self.network_model {
            // In asynchrony we advance immediately.
            NetworkModel::Asynchronous => true,

            // In partial synchrony, we need to wait for the leader or for enough votes.
            NetworkModel::PartiallySynchronous => match self.round % 2 {
                0 => self.update_leader(),
                _ => self.enough_votes(),
            },
        }
    }

    /// Main loop listening to incoming messages.
    pub async fn run(&mut self) {
        debug!("Dag starting at round {}", self.round);
        let mut advance = true;

        let timer = sleep(self.max_header_delay);
        tokio::pin!(timer);

        info!("Proposer on node {} has started successfully.", self.name);
        loop {
            // Check if we can propose a new header. We propose a new header when we have a quorum of parents
            // and one of the following conditions is met:
            // (i) the timer expired (we timed out on the leader or gave up gather votes for the leader),
            // (ii) we have enough digests (header_num_of_batches_threshold) and we are on the happy path (we can vote for
            // the leader or the leader has enough votes to enable a commit). The latter condition only matters
            // in partially synchrony. We guarantee that no more than max_header_num_of_batches are included in
            let enough_parents = !self.last_parents.is_empty();
            let enough_digests = self.digests.len() >= self.header_num_of_batches_threshold;
            let mut timer_expired = timer.is_elapsed();

            if (timer_expired || (enough_digests && advance)) && enough_parents {
                if timer_expired && matches!(self.network_model, NetworkModel::PartiallySynchronous)
                {
                    // It is expected that this timer expires from time to time. If it expires too often, it
                    // either means some validators are Byzantine or that the network is experiencing periods
                    // of asynchrony. In practice, the latter scenario means we misconfigured the parameter
                    // called `max_header_delay`.
                    debug!("Timer expired for round {}", self.round);
                }

                // Advance to the next round.
                self.round += 1;
                self.metrics
                    .current_round
                    .with_label_values(&[&self.committee.epoch.to_string()])
                    .set(self.round as i64);
                debug!("Dag moved to round {}", self.round);

                // Make a new header.
                match self.make_header().await {
                    Err(e @ DagError::ShuttingDown) => debug!("{e}"),
                    Err(e) => panic!("Unexpected error: {e}"),
                    Ok(digests) => {
                        let reason = if timer_expired {
                            "timeout"
                        } else {
                            "threshold_size_reached"
                        };

                        self.metrics
                            .num_of_batch_digests_in_header
                            .with_label_values(&[&self.committee.epoch.to_string(), reason])
                            .observe(digests as f64);
                    }
                }

                // Reschedule the timer.
                let deadline = self.timeout_value();
                timer.as_mut().reset(deadline);
                timer_expired = false;
            }

            tokio::select! {
                Some((parents, round, epoch)) = self.rx_parents.recv() => {
                    // If the core already moved to the next epoch we should pull the next
                    // committee as well.
                    match epoch.cmp(&self.committee.epoch()) {
                        Ordering::Greater => {
                            let message = self.rx_reconfigure.borrow_and_update().clone();
                            match message  {
                                ReconfigureNotification::NewEpoch(new_committee) => {
                                    self.change_epoch(new_committee);
                                },
                                ReconfigureNotification::UpdateCommittee(new_committee) => {
                                    self.committee = new_committee;
                                },
                                ReconfigureNotification::Shutdown => return,
                            }
                            tracing::debug!("Committee updated to {}", self.committee);
                        }
                        Ordering::Less => {
                            // We already updated committee but the core is slow. Ignore the parents
                            // from older epochs.
                            continue
                        },
                        Ordering::Equal => {
                            // Nothing to do, we can proceed.
                        }
                    }

                    // Compare the parents' round number with our current round.
                    match round.cmp(&self.round) {
                        Ordering::Greater => {
                            // We accept round bigger than our current round to jump ahead in case we were
                            // late (or just joined the network).
                            self.round = round;
                            self.last_parents = parents;

                        },
                        Ordering::Less => {
                            // Ignore parents from older rounds.
                            continue;
                        },
                        Ordering::Equal => {
                            // The core gives us the parents the first time they are enough to form a quorum.
                            // Then it keeps giving us all the extra parents.
                            self.last_parents.extend(parents)
                        }
                    }

                    // Check whether we can advance to the next round. Note that if we timeout,
                    // we ignore this check and advance anyway.
                    advance = self.ready();

                    let round_type = if self.round % 2 == 0 {
                        "even"
                    } else {
                        "odd"
                    };

                    self.metrics
                    .proposer_ready_to_advance
                    .with_label_values(&[&self.committee.epoch.to_string(), &advance.to_string(), round_type])
                    .inc();
                }

                // Receive digests from our workers.
                Some(digest_record) = self.rx_our_digests.recv() => {
                    self.digests.push(digest_record);
                }

                // Check whether the timer expired.
                () = &mut timer, if !timer_expired => {
                    // Nothing to do.
                }

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewEpoch(new_committee) => {
                            self.change_epoch(new_committee);
                        },
                        ReconfigureNotification::UpdateCommittee(new_committee) => {
                            self.committee = new_committee;
                        },
                        ReconfigureNotification::Shutdown => return,
                    }
                    tracing::debug!("Committee updated to {}", self.committee);

                }
            }

            // update metrics
            self.metrics
                .num_of_pending_batches_in_proposer
                .with_label_values(&[&self.committee.epoch.to_string()])
                .set(self.digests.len() as i64);
        }
    }
}
