// Copyright(C) Facebook, Inc. and its affiliates.
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{SharedCommittee, WorkerId};
use crypto::{traits::VerifyingKey, Digest, Hash as _, SignatureService};
use std::cmp::Ordering;
use tokio::{
    sync::mpsc::{Receiver, Sender},
    time::{sleep, Duration, Instant},
};
#[cfg(feature = "benchmark")]
use tracing::info;
use tracing::{debug, warn};
use types::{BatchDigest, Certificate, Header, Round};

#[cfg(test)]
#[path = "tests/part_sync_proposer_tests.rs"]
pub mod part_sync_proposer_tests;

/// The proposer creates new headers and send them to the core for broadcasting and further processing.
pub struct PartiallySyncProposer<PublicKey: VerifyingKey> {
    /// The public key of this primary.
    name: PublicKey,
    /// The committee information.
    committee: SharedCommittee<PublicKey>,
    /// Service to sign headers.
    signature_service: SignatureService<PublicKey::Sig>,
    /// The size of the headers' payload.
    header_size: usize,
    /// The maximum delay to wait for batches' digests.
    max_header_delay: Duration,

    /// Receives the parents to include in the next header (along with their round number).
    rx_core: Receiver<(Vec<Certificate<PublicKey>>, Round)>,
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
}

impl<PublicKey: VerifyingKey> PartiallySyncProposer<PublicKey> {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        name: PublicKey,
        committee: SharedCommittee<PublicKey>,
        signature_service: SignatureService<PublicKey::Sig>,
        header_size: usize,
        max_header_delay: Duration,
        rx_core: Receiver<(Vec<Certificate<PublicKey>>, Round)>,
        rx_workers: Receiver<(BatchDigest, WorkerId)>,
        tx_core: Sender<Header<PublicKey>>,
    ) {
        let genesis = Certificate::genesis(&committee);
        tokio::spawn(async move {
            Self {
                name,
                committee,
                signature_service,
                header_size,
                max_header_delay,
                rx_core,
                rx_workers,
                tx_core,
                round: 0,
                last_parents: genesis,
                last_leader: None,
                digests: Vec::with_capacity(2 * header_size),
                payload_size: 0,
            }
            .run()
            .await;
        });
    }

    async fn make_header(&mut self) {
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
        debug!("Created {:?}", header);

        #[cfg(feature = "benchmark")]
        for digest in header.payload.keys() {
            // NOTE: This log entry is used to compute performance.
            info!("Created {} -> {:?}", header, digest);
        }

        // Send the new header to the `Core` that will broadcast and process it.
        self.tx_core
            .send(header)
            .await
            .expect("Failed to send header");
    }

    /// Update the last leader certificate.
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
    /// or (iii) there is no leader to vote for.
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
            // the leader or the leader has enough votes to enable a commit).
            let enough_parents = !self.last_parents.is_empty();
            let enough_digests = self.payload_size >= self.header_size;
            let timer_expired = timer.is_elapsed();

            if (timer_expired || (enough_digests && advance)) && enough_parents {
                if timer_expired {
                    warn!("Timer expired for round {}", self.round);
                }

                // Advance to the next round.
                self.round += 1;
                debug!("Dag moved to round {}", self.round);

                // Make a new header.
                self.make_header().await;
                self.payload_size = 0;

                // Reschedule the timer.
                let deadline = Instant::now() + self.max_header_delay;
                timer.as_mut().reset(deadline);
            }

            tokio::select! {
                Some((parents, round)) = self.rx_core.recv() => {
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
                        },
                        Ordering::Equal => {
                            // The core gives us the parents the first time they are enough to form a quorum.
                            // Then it keeps giving us all the extra parents.
                            self.last_parents.extend(parents)
                        }
                    }

                    // Check whether we can advance to the next round. Note that if we timeout,
                    // we ignore this check and advance anyway.
                    advance = match self.round % 2 {
                        0 => self.update_leader(),
                        _ => self.enough_votes(),
                    }
                }
                Some((digest, worker_id)) = self.rx_workers.recv() => {
                    self.payload_size += Digest::from(digest).size();
                    self.digests.push((digest, worker_id));
                }
                () = &mut timer => {
                    // Nothing to do.
                }
            }
        }
    }
}
