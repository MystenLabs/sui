// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::metrics::ConsensusMetrics;
use crate::{
    consensus::{ConsensusProtocol, ConsensusState, Dag},
    utils, ConsensusOutput,
};
use config::{Committee, Stake};
use fastcrypto::{hash::Hash, traits::EncodeDecodeBase64};
use std::{collections::BTreeSet, sync::Arc};
use tokio::time::Instant;
use tracing::{debug, error};
use types::{Certificate, CertificateDigest, ConsensusStore, Round, SequenceNumber, StoreResult};

#[cfg(test)]
#[path = "tests/bullshark_tests.rs"]
pub mod bullshark_tests;

#[derive(Default)]
struct LastRound {
    _round: Round,
    leader_found: bool,
    leader_has_support: bool,
}

pub struct Bullshark {
    /// The committee information.
    pub committee: Committee,
    /// Persistent storage to safe ensure crash-recovery.
    pub store: Arc<ConsensusStore>,
    /// The depth of the garbage collector.
    pub gc_depth: Round,

    pub metrics: Arc<ConsensusMetrics>,
    /// The last time we had a successful leader election
    /// which had enough support
    last_leader_election_timestamp: Instant,
    /// The last round for which a successful leader election took place
    last_election_round: LastRound,
    /// The most recent round of inserted certificate
    max_inserted_certificate_round: Round,
}

impl ConsensusProtocol for Bullshark {
    fn process_certificate(
        &mut self,
        state: &mut ConsensusState,
        consensus_index: SequenceNumber,
        certificate: Certificate,
    ) -> StoreResult<Vec<ConsensusOutput>> {
        debug!("Processing {:?}", certificate);
        let round = certificate.round();
        let mut consensus_index = consensus_index;

        // We must have stored already the parents of this certificate!
        self.check_parents_exist(&certificate, state);

        // Add the new certificate to the local storage.
        if state.try_insert(certificate).is_err() {
            return Ok(Vec::new());
        }

        if round != self.max_inserted_certificate_round && round % 2 == 0 {
            // check when the last non successful leader election was - if it is != the round-2
            // then report an unsuccessful election
            let last_round = &self.last_election_round;

            if !last_round.leader_found {
                // increase the leader not found metric
                self.metrics.leader_not_found.inc();
            } else if !last_round.leader_has_support {
                // increase the leader not enough support
                self.metrics.leader_not_enough_support.inc();
            }
        }

        self.max_inserted_certificate_round = self.max_inserted_certificate_round.max(round);

        // Try to order the dag to commit. Start from the highest round for which we have at least
        // f+1 certificates. This is because we need them to reveal the common coin.
        let r = round - 1;

        // We only elect leaders for even round numbers.
        if r % 2 != 0 || r < 2 {
            return Ok(Vec::new());
        }

        // Get the certificate's digest of the leader. If we already ordered this leader,
        // there is nothing to do.
        let leader_round = r;
        if leader_round <= state.last_committed_round {
            return Ok(Vec::new());
        }
        let (leader_digest, leader) = match Self::leader(&self.committee, leader_round, &state.dag)
        {
            Some(x) => x,
            None => {
                self.last_election_round = LastRound {
                    _round: leader_round,
                    leader_found: false,
                    leader_has_support: false,
                };
                // leader has not been found - we don't have any certificate
                return Ok(Vec::new());
            }
        };

        // Check if the leader has f+1 support from its children (ie. round r-1).
        let stake: Stake = state
            .dag
            .get(&round)
            .expect("We should have the whole history by now")
            .values()
            .filter(|(_, x)| x.header.parents.contains(leader_digest))
            .map(|(_, x)| self.committee.stake(&x.origin()))
            .sum();

        self.last_election_round = LastRound {
            _round: leader_round,
            leader_found: true,
            leader_has_support: false,
        };

        // If it is the case, we can commit the leader. But first, we need to recursively go back to
        // the last committed leader, and commit all preceding leaders in the right order. Committing
        // a leader block means committing all its dependencies.
        if stake < self.committee.validity_threshold() {
            debug!("Leader {:?} does not have enough support", leader);
            return Ok(Vec::new());
        }

        self.last_election_round.leader_has_support = true;

        // Get an ordered list of past leaders that are linked to the current leader.
        debug!("Leader {:?} has enough support", leader);
        let mut sequence = Vec::new();

        // TODO: duplicated in tusk.rs
        for leader in utils::order_leaders(&self.committee, leader, state, Self::leader)
            .iter()
            .rev()
        {
            debug!("Previous Leader {:?} has enough support", leader);

            // Starting from the oldest leader, flatten the sub-dag referenced by the leader.
            for x in utils::order_dag(self.gc_depth, leader, state) {
                let digest = x.digest();

                // Update and clean up internal state.
                state.update(&x, self.gc_depth);

                // Add the certificate to the sequence.
                sequence.push(ConsensusOutput {
                    certificate: x,
                    consensus_index,
                });

                // Increase the global consensus index.
                consensus_index += 1;

                // Persist the update.
                // TODO [issue #116]: Ensure this is not a performance bottleneck.
                self.store.write_consensus_state(
                    &state.last_committed,
                    &consensus_index,
                    &digest,
                )?;
            }
        }

        // record the last time we got a successful leader election
        let elapsed = self.last_leader_election_timestamp.elapsed();

        self.metrics
            .commit_rounds_latency
            .observe(elapsed.as_secs_f64());

        self.last_leader_election_timestamp = Instant::now();

        self.metrics.leader_found.inc();

        // Log the latest committed round of every authority (for debug).
        // Performance note: if tracing at the debug log level is disabled, this is cheap, see
        // https://github.com/tokio-rs/tracing/pull/326
        for (name, round) in &state.last_committed {
            debug!("Latest commit of {}: Round {}", name.encode_base64(), round);
        }

        debug!("Total committed certificates: {}", sequence.len());

        self.metrics.commit_depth.observe(sequence.len() as f64);

        Ok(sequence)
    }

    fn update_committee(&mut self, new_committee: Committee) -> StoreResult<()> {
        self.committee = new_committee;
        self.store.clear()
    }
}

impl Bullshark {
    /// Create a new Bullshark consensus instance.
    pub fn new(
        committee: Committee,
        store: Arc<ConsensusStore>,
        gc_depth: Round,
        metrics: Arc<ConsensusMetrics>,
    ) -> Self {
        Self {
            committee,
            store,
            gc_depth,
            last_leader_election_timestamp: Instant::now(),
            last_election_round: LastRound::default(),
            max_inserted_certificate_round: 0,
            metrics,
        }
    }

    // Checks that the provided certificate's parents exist and prints the necessary
    // log statements. This method does not take more actions other than printing
    // log statements.
    fn check_parents_exist(&mut self, certificate: &Certificate, state: &ConsensusState) {
        let round = certificate.round();
        if round > 0 {
            let parents = certificate.header.parents.clone();
            if let Some(round_table) = state.dag.get(&(round - 1)) {
                let store_parents: BTreeSet<&CertificateDigest> =
                    round_table.iter().map(|(_, (digest, _))| digest).collect();

                for parent_digest in parents {
                    if !store_parents.contains(&parent_digest) {
                        if round - 1 + self.gc_depth > state.last_committed_round {
                            error!(
                                "The store does not contain the parent of {:?}: Missing item digest={:?}",
                                certificate, parent_digest
                            );
                        } else {
                            debug!(
                                "The store does not contain the parent of {:?}: Missing item digest={:?} (but below GC round)",
                                certificate, parent_digest
                            );
                        }
                    }
                }
            } else {
                error!(
                    "Round not present in Dag store: {:?} when looking for parents of {:?}",
                    round - 1,
                    certificate
                );
            }
        }
    }

    // TODO: duplicated in tusk.rs
    /// Returns the certificate (and the certificate's digest) originated by the leader of the
    /// specified round (if any).
    fn leader<'a>(
        committee: &Committee,
        round: Round,
        dag: &'a Dag,
    ) -> Option<&'a (CertificateDigest, Certificate)> {
        // Note: this function is often called with even rounds only. While we do not aim at random selection
        // yet (see issue #10), repeated calls to this function should still pick from the whole roster of leaders.

        cfg_if::cfg_if! {
            if #[cfg(test)] {
                // consensus tests rely on returning the same leader.
                let leader = committee.authorities.iter().next().expect("Empty authorities table!").0;
            } else {
                // Elect the leader in a stake-weighted choice seeded by the round
                let leader = &committee.leader(round);
            }
        }

        // Return its certificate and the certificate's digest.
        dag.get(&round).and_then(|x| x.get(leader))
    }
}
