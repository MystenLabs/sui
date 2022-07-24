// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{metrics::PrimaryMetrics, NetworkModel};
use config::{Committee, Epoch, WorkerId};
use crypto::{traits::VerifyingKey, Digest, Hash as _, SignatureService};
use std::{cmp::Ordering, sync::Arc};
use tokio::{
    sync::{
        mpsc::{Receiver, Sender},
        watch,
    },
    task::JoinHandle,
    time::{sleep, Duration, Instant},
};
use tracing::debug;
use types::{
    error::{DagError, DagResult},
    BatchDigest, Certificate, Header, ReconfigureNotification, Round,
};

#[cfg(test)]
#[path = "tests/proposer_tests.rs"]
pub mod proposer_tests;

/// The proposer creates new headers and send them to the core for broadcasting and further processing.
pub struct Proposer<PublicKey: VerifyingKey> {
    /// The public key of this primary.
    name: PublicKey,
    /// The committee information.
    committee: Committee<PublicKey>,
    /// Service to sign headers.
    signature_service: SignatureService<PublicKey::Sig>,
    /// The size of the headers' payload.
    header_size: usize,
    /// The maximum delay to wait for batches' digests.
    max_header_delay: Duration,
    /// The network model in which the node operates.
    network_model: NetworkModel,

    /// Watch channel to reconfigure the committee.
    rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
    /// Receives the parents to include in the next header (along with their round number).
    rx_core: Receiver<(Vec<Certificate<PublicKey>>, Round, Epoch)>,
    /// Receives the batches' digests from our workers.
    rx_workers: Receiver<(BatchDigest, WorkerId)>,
    /// Sends newly created headers to the `Core`.
    tx_core: Sender<Header<PublicKey>>,

    /// The current round of the dag.
    round: Round,
    /// Holds the certificates' ids waiting to be included in the next header.
    last_parents: Vec<Certificate<PublicKey>>,
    /// Holds the certificate of the last leader (if any).
    last_leader: Option<Certificate<PublicKey>>,
    /// Holds the batches' digests waiting to be included in the next header.
    digests: Vec<(BatchDigest, WorkerId)>,
    /// Keeps track of the size (in bytes) of batches' digests that we received so far.
    payload_size: usize,
    /// Metrics handler
    metrics: Arc<PrimaryMetrics>,
}

impl<PublicKey: VerifyingKey> Proposer<PublicKey> {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        committee: Committee<PublicKey>,
        signature_service: SignatureService<PublicKey::Sig>,
        header_size: usize,
        max_header_delay: Duration,
        network_model: NetworkModel,
        rx_reconfigure: watch::Receiver<ReconfigureNotification<PublicKey>>,
        rx_core: Receiver<(Vec<Certificate<PublicKey>>, Round, Epoch)>,
        rx_workers: Receiver<(BatchDigest, WorkerId)>,
        tx_core: Sender<Header<PublicKey>>,
        metrics: Arc<PrimaryMetrics>,
    ) -> JoinHandle<()> {
        let genesis = Certificate::genesis(&committee);
        tokio::spawn(async move {
            Self {
                name,
                committee,
                signature_service,
                header_size,
                max_header_delay,
                network_model,
                rx_reconfigure,
                rx_core,
                rx_workers,
                tx_core,
                round: 0,
                last_parents: genesis,
                last_leader: None,
                digests: Vec::with_capacity(2 * header_size),
                payload_size: 0,
                metrics,
            }
            .run()
            .await;
        })
    }

    async fn make_header(&mut self) -> DagResult<()> {
        // Make a new header.
        let header = Header::new(
            self.name.clone(),
            self.round,
            self.committee.epoch(),
            self.digests.drain(..).collect(),
            self.last_parents.drain(..).map(|x| x.digest()).collect(),
            &mut self.signature_service,
        )
        .await;
        debug!("Created {header:?}");

        #[cfg(feature = "benchmark")]
        for digest in header.payload.keys() {
            // NOTE: This log entry is used to compute performance.
            tracing::info!("Created {} -> {:?}", header, digest);
        }

        // Send the new header to the `Core` that will broadcast and process it.
        self.tx_core
            .send(header)
            .await
            .map_err(|_| DagError::ShuttingDown)
    }

    /// Update the committee and cleanup internal state.
    fn update_committee(&mut self, committee: Committee<PublicKey>) {
        self.committee = committee;
        self.round = 0;
        self.last_parents = Certificate::genesis(&self.committee);
        tracing::debug!("Committee updated to {}", self.committee);
    }

    // Main loop listening to incoming messages.
    /// Update the last leader certificate. This is only relevant in partial synchrony.
    fn update_leader(&mut self) -> bool {
        let leader_name = self.committee.leader(self.round as usize);
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

    /// Check whether if we have (i) 2f+1 votes for the leader, (ii) f+1 nodes not voting for the leader,
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

        let mut enough_votes = votes_for_leader >= self.committee.quorum_threshold();
        if enough_votes {
            if let Some(leader) = self.last_leader.as_ref() {
                debug!(
                    "Got enough support for leader {} at round {}",
                    leader.origin(),
                    self.round
                );
            }
        }
        enough_votes |= no_votes >= self.committee.validity_threshold();
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

        loop {
            // Check if we can propose a new header. We propose a new header when we have a quorum of parents
            // and one of the following conditions is met:
            // (i) the timer expired (we timed out on the leader or gave up gather votes for the leader),
            // (ii) we have enough digests (minimum header size) and we are on the happy path (we can vote for
            // the leader or the leader has enough votes to enable a commit). The latter condition only matters
            // in partially synchrony.
            let enough_parents = !self.last_parents.is_empty();
            let enough_digests = self.payload_size >= self.header_size;
            let timer_expired = timer.is_elapsed();

            if (timer_expired || (enough_digests && advance)) && enough_parents {
                if timer_expired && matches!(self.network_model, NetworkModel::PartiallySynchronous)
                {
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
                    Ok(()) => (),
                }
                self.payload_size = 0;

                // Reschedule the timer.
                let deadline = Instant::now() + self.max_header_delay;
                timer.as_mut().reset(deadline);
            }

            tokio::select! {
                Some((parents, round, epoch)) = self.rx_core.recv() => {
                    // If the core already moved to the next epoch we should pull the next
                    // committee as well.
                    match epoch.cmp(&self.committee.epoch()) {
                        Ordering::Greater => {
                            let message = self.rx_reconfigure.borrow_and_update().clone();
                            match message  {
                                ReconfigureNotification::NewCommittee(new_committee) => {
                                    self.update_committee(new_committee);
                                },
                                ReconfigureNotification::Shutdown => return,
                            }
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
                }

                // Receive digests from our workers.
                Some((digest, worker_id)) = self.rx_workers.recv() => {
                    self.payload_size += Digest::from(digest).size();
                    self.digests.push((digest, worker_id));
                }

                // Check whether the timer expired.
                () = &mut timer => {
                    // Nothing to do.
                }

                // Check whether the committee changed.
                result = self.rx_reconfigure.changed() => {
                    result.expect("Committee channel dropped");
                    let message = self.rx_reconfigure.borrow().clone();
                    match message {
                        ReconfigureNotification::NewCommittee(new_committee) => {
                            self.update_committee(new_committee);
                        },
                        ReconfigureNotification::Shutdown => return,
                    }
                }
            }
        }
    }
}
