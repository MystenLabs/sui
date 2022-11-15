// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    consensus::{ConsensusProtocol, ConsensusState, Dag},
    utils, SequenceNumber,
};
use config::{Committee, Stake};
use fastcrypto::{hash::Hash, traits::EncodeDecodeBase64};
use std::{collections::HashMap, sync::Arc};
use tracing::debug;
use types::{
    Certificate, CertificateDigest, CommittedSubDag, ConsensusOutput, ConsensusStore, Round,
    StoreResult,
};

#[cfg(any(test))]
#[path = "tests/tusk_tests.rs"]
pub mod tusk_tests;

pub struct Tusk {
    /// The committee information.
    pub committee: Committee,
    /// Persistent storage to safe ensure crash-recovery.
    pub store: Arc<ConsensusStore>,
    /// The depth of the garbage collector.
    pub gc_depth: Round,
}

impl ConsensusProtocol for Tusk {
    fn process_certificate(
        &mut self,
        state: &mut ConsensusState,
        consensus_index: SequenceNumber,
        certificate: Certificate,
    ) -> StoreResult<Vec<CommittedSubDag>> {
        debug!("Processing {:?}", certificate);
        let round = certificate.round();
        let mut consensus_index = consensus_index;

        // Add the new certificate to the local storage.
        state
            .dag
            .entry(round)
            .or_insert_with(HashMap::new)
            .insert(certificate.origin(), (certificate.digest(), certificate));

        // Try to order the dag to commit. Start from the highest round for which we have at least
        // 2f+1 certificates. This is because we need them to reveal the common coin.
        let r = round - 1;

        // We only elect leaders for even round numbers.
        if r % 2 != 0 || r < 4 {
            return Ok(Vec::new());
        }

        // Get the certificate's digest of the leader of round r-2. If we already ordered this leader,
        // there is nothing to do.
        let leader_round = r - 2;
        if leader_round <= state.last_committed_round {
            return Ok(Vec::new());
        }
        let (leader_digest, leader) = match Self::leader(&self.committee, leader_round, &state.dag)
        {
            Some(x) => x,
            None => return Ok(Vec::new()),
        };

        // Check if the leader has f+1 support from its children (ie. round r-1).
        let stake: Stake = state
            .dag
            .get(&(r - 1))
            .expect("We should have the whole history by now")
            .values()
            .filter(|(_, x)| x.header.parents.contains(leader_digest))
            .map(|(_, x)| self.committee.stake(&x.origin()))
            .sum();

        // If it is the case, we can commit the leader. But first, we need to recursively go back to
        // the last committed leader, and commit all preceding leaders in the right order. Committing
        // a leader block means committing all its dependencies.
        if stake < self.committee.validity_threshold() {
            debug!("Leader {:?} does not have enough support", leader);
            return Ok(Vec::new());
        }

        // Get an ordered list of past leaders that are linked to the current leader.
        debug!("Leader {:?} has enough support", leader);
        let mut committed_sub_dags = Vec::new();

        for leader in utils::order_leaders(&self.committee, leader, state, Self::leader)
            .iter()
            .rev()
        {
            let mut sequence = Vec::new();

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

            committed_sub_dags.push(CommittedSubDag {
                certificates: sequence,
                leader: leader.clone(),
            });
        }

        // Log the latest committed round of every authority (for debug).
        // Performance note: if tracing at the debug log level is disabled, this is cheap, see
        // https://github.com/tokio-rs/tracing/pull/326
        for (name, round) in &state.last_committed {
            debug!("Latest commit of {}: Round {}", name.encode_base64(), round);
        }

        Ok(committed_sub_dags)
    }

    fn update_committee(&mut self, new_committee: Committee) -> StoreResult<()> {
        self.committee = new_committee;
        self.store.clear()
    }
}

impl Tusk {
    /// Create a new Tusk consensus instance.
    pub fn new(committee: Committee, store: Arc<ConsensusStore>, gc_depth: Round) -> Self {
        Self {
            committee,
            store,
            gc_depth,
        }
    }

    /// Returns the certificate (and the certificate's digest) originated by the leader of the
    /// specified round (if any).
    fn leader<'a>(
        committee: &Committee,
        round: Round,
        dag: &'a Dag,
    ) -> Option<&'a (CertificateDigest, Certificate)> {
        // TODO: We should elect the leader of round r-2 using the common coin revealed at round r.
        // At this stage, we are guaranteed to have 2f+1 certificates from round r (which is enough to
        // compute the coin). We currently just use a stake-weighted choise seeded by the round.
        //
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::ConsensusMetrics;
    use arc_swap::ArcSwap;
    use prometheus::Registry;
    use rand::Rng;
    use std::collections::BTreeSet;
    use test_utils::{make_consensus_store, CommitteeFixture};
    use types::Certificate;

    #[tokio::test]
    async fn state_limits_test() {
        let gc_depth = 12;
        let rounds: Round = rand::thread_rng().gen_range(10..100);

        // process certificates for rounds, check we don't grow the dag too much
        let fixture = CommitteeFixture::builder().build();
        let committee = fixture.committee();
        let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();

        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) =
            test_utils::make_optimal_certificates(&committee, 1..=rounds, &genesis, &keys);

        let store_path = test_utils::temp_dir();
        let store = make_consensus_store(&store_path);

        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

        let consensus_index = 0;
        let mut state = ConsensusState::new(Certificate::genesis(&committee), metrics);
        let mut tusk = Tusk::new(committee, store, gc_depth);
        for certificate in certificates {
            tusk.process_certificate(&mut state, consensus_index, certificate)
                .unwrap();
        }
        // with "optimal" certificates (see `make_optimal_certificates`), and a round-robin between leaders,
        // we need at most 6 rounds lookbehind: we elect a leader at most at round r-2, and its round is
        // preceded by one round of history for each prior leader, which contains their latest commit at least.
        //
        // -- L1's latest
        // -- L2's latest
        // -- L3's latest
        // -- L4's latest
        // -- support level 1 (for L4)
        // -- support level 2 (for L4)
        //
        let n = state.dag.len();
        assert!(n <= 6, "DAG size: {}", n);
    }

    #[tokio::test]
    async fn imperfect_state_limits_test() {
        let gc_depth = 12;
        let rounds: Round = rand::thread_rng().gen_range(10..100);

        // process certificates for rounds, check we don't grow the dag too much
        let fixture = CommitteeFixture::builder().build();
        let committee = fixture.committee();
        let keys: Vec<_> = fixture.authorities().map(|a| a.public_key()).collect();

        let genesis = Certificate::genesis(&committee)
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        // TODO: evidence that this test fails when `failure_probability` parameter >= 1/3
        let (certificates, _next_parents) =
            test_utils::make_certificates(&committee, 1..=rounds, &genesis, &keys, 0.333);
        let arc_committee = Arc::new(ArcSwap::from_pointee(committee.clone()));

        let store_path = test_utils::temp_dir();
        let store = make_consensus_store(&store_path);

        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

        let mut state = ConsensusState::new(Certificate::genesis(&committee), metrics);
        let consensus_index = 0;
        let mut tusk = Tusk::new((**arc_committee.load()).clone(), store, gc_depth);

        for certificate in certificates {
            tusk.process_certificate(&mut state, consensus_index, certificate)
                .unwrap();
        }

        // with "less optimal" certificates (see `make_certificates`), we should keep at most gc_depth rounds lookbehind
        let n = state.dag.len();
        assert!(n <= gc_depth as usize, "DAG size: {}", n);
    }
}
