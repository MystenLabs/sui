// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::{
    consensus::{ConsensusProtocol, ConsensusState, Dag},
    utils, ConsensusOutput,
};
use config::{Committee, Stake};
use crypto::{
    traits::{EncodeDecodeBase64, VerifyingKey},
    Hash,
};
use std::{collections::HashMap, sync::Arc};
use tracing::debug;
use types::{Certificate, CertificateDigest, ConsensusStore, Round, SequenceNumber, StoreResult};

#[cfg(test)]
#[path = "tests/bullshark_tests.rs"]
pub mod bullshark_tests;

pub struct Bullshark<PublicKey: VerifyingKey> {
    /// The committee information.
    pub committee: Committee<PublicKey>,
    /// Persistent storage to safe ensure crash-recovery.
    pub store: Arc<ConsensusStore<PublicKey>>,
    /// The depth of the garbage collector.
    pub gc_depth: Round,
}

impl<PublicKey: VerifyingKey> ConsensusProtocol<PublicKey> for Bullshark<PublicKey> {
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
            None => return Ok(Vec::new()),
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

impl<PublicKey: VerifyingKey> Bullshark<PublicKey> {
    /// Create a new Bullshark consensus instance.
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
        #[cfg(test)]
        let seed = 0;
        #[cfg(not(test))]
        let seed = round;

        // Elect the leader in a round-robin fashion.
        let leader = committee.leader(seed as usize);

        // Return its certificate and the certificate's digest.
        dag.get(&round).and_then(|x| x.get(&leader))
    }
}
