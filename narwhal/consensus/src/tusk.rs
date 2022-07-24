// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    consensus::{ConsensusProtocol, ConsensusState, Dag},
    utils, ConsensusOutput, SequenceNumber,
};
use config::{Committee, Stake};
use crypto::{
    traits::{EncodeDecodeBase64, VerifyingKey},
    Hash,
};
use std::{collections::HashMap, sync::Arc};
use tracing::debug;
use types::{Certificate, CertificateDigest, ConsensusStore, Round, StoreResult};

#[cfg(any(test))]
#[path = "tests/tusk_tests.rs"]
pub mod tusk_tests;

pub struct Tusk<PublicKey: VerifyingKey> {
    /// The committee information.
    pub committee: Committee<PublicKey>,
    /// Persistent storage to safe ensure crash-recovery.
    pub store: Arc<ConsensusStore<PublicKey>>,
    /// The depth of the garbage collector.
    pub gc_depth: Round,
}

impl<PublicKey: VerifyingKey> ConsensusProtocol<PublicKey> for Tusk<PublicKey> {
    fn process_certificate(
        &mut self,
        state: &mut ConsensusState<PublicKey>,
        consensus_index: SequenceNumber,
        certificate: Certificate<PublicKey>,
    ) -> StoreResult<Vec<ConsensusOutput<PublicKey>>> {
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
        let mut sequence = Vec::new();
        for leader in utils::order_leaders(&self.committee, leader, state, Self::leader)
            .iter()
            .rev()
        {
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

        // Log the latest committed round of every authority (for debug).
        // Performance note: if tracing at the debug log level is disabled, this is cheap, see
        // https://github.com/tokio-rs/tracing/pull/326
        for (name, round) in &state.last_committed {
            debug!("Latest commit of {}: Round {}", name.encode_base64(), round);
        }

        Ok(sequence)
    }

    fn update_committee(&mut self, new_committee: Committee<PublicKey>) -> StoreResult<()> {
        self.committee = new_committee;
        self.store.clear()
    }
}

impl<PublicKey: VerifyingKey> Tusk<PublicKey> {
    /// Create a new Tusk consensus instance.
    pub fn new(
        committee: Committee<PublicKey>,
        store: Arc<ConsensusStore<PublicKey>>,
        gc_depth: Round,
    ) -> Self {
        Self {
            committee,
            store,
            gc_depth,
        }
    }

    /// Returns the certificate (and the certificate's digest) originated by the leader of the
    /// specified round (if any).
    fn leader<'a>(
        committee: &Committee<PublicKey>,
        round: Round,
        dag: &'a Dag<PublicKey>,
    ) -> Option<&'a (CertificateDigest, Certificate<PublicKey>)> {
        // TODO: We should elect the leader of round r-2 using the common coin revealed at round r.
        // At this stage, we are guaranteed to have 2f+1 certificates from round r (which is enough to
        // compute the coin). We currently just use round-robin.
        #[cfg(test)]
        let coin = 0;
        #[cfg(not(test))]
        let coin = round as usize;

        // Elect the leader.
        let leader = committee.leader(coin);

        // Return its certificate and the certificate's digest.
        dag.get(&round).and_then(|x| x.get(&leader))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::ConsensusMetrics;
    use arc_swap::ArcSwap;
    use crypto::traits::KeyPair;
    use prometheus::Registry;
    use rand::Rng;
    use std::collections::BTreeSet;
    use test_utils::{make_consensus_store, mock_committee};
    use types::Certificate;

    #[test]
    fn state_limits_test() {
        let gc_depth = 12;
        let rounds: Round = rand::thread_rng().gen_range(10, 100);

        // process certificates for rounds, check we don't grow the dag too much
        let keys: Vec<_> = test_utils::keys(None)
            .into_iter()
            .map(|kp| kp.public().clone())
            .collect();

        let genesis = Certificate::genesis(&mock_committee(&keys[..]))
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) =
            test_utils::make_optimal_certificates(1..=rounds, &genesis, &keys);
        let committee = mock_committee(&keys);

        let store_path = test_utils::temp_dir();
        let store = make_consensus_store(&store_path);

        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

        let consensus_index = 0;
        let mut state =
            ConsensusState::new(Certificate::genesis(&mock_committee(&keys[..])), metrics);
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

    #[test]
    fn imperfect_state_limits_test() {
        let gc_depth = 12;
        let rounds: Round = rand::thread_rng().gen_range(10, 100);

        // process certificates for rounds, check we don't grow the dag too much
        let keys: Vec<_> = test_utils::keys(None)
            .into_iter()
            .map(|kp| kp.public().clone())
            .collect();

        let genesis = Certificate::genesis(&mock_committee(&keys[..]))
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        // TODO: evidence that this test fails when `failure_probability` parameter >= 1/3
        let (certificates, _next_parents) =
            test_utils::make_certificates(1..=rounds, &genesis, &keys, 0.333);
        let committee = Arc::new(ArcSwap::from_pointee(mock_committee(&keys)));

        let store_path = test_utils::temp_dir();
        let store = make_consensus_store(&store_path);

        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));

        let mut state =
            ConsensusState::new(Certificate::genesis(&mock_committee(&keys[..])), metrics);
        let consensus_index = 0;
        let mut tusk = Tusk::new((**committee.load()).clone(), store, gc_depth);

        for certificate in certificates {
            tusk.process_certificate(&mut state, consensus_index, certificate)
                .unwrap();
        }

        // with "less optimal" certificates (see `make_certificates`), we should keep at most gc_depth rounds lookbehind
        let n = state.dag.len();
        assert!(n <= gc_depth as usize, "DAG size: {}", n);
    }
}
