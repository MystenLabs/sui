// Copyright(C) Facebook, Inc. and its affiliates.
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

use arrayvec::ArrayVec;
use config::{Committee, Stake};
use crypto::{Digest, Hash as _, PublicKey};
use eyre::Result;
use log::{debug, info, log_enabled, warn};
use primary::{Certificate, Round};
use serde::{Deserialize, Serialize};
use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    convert::TryInto,
    path::Path,
};
use store::{rocks::DBMap, Map};
use tokio::sync::mpsc::{Receiver, Sender};

#[cfg(any(test, feature = "benchmark"))]
#[path = "tests/consensus_tests.rs"]
pub mod consensus_tests;

/// A concatenated Round + Pubkey key for "flattened" database use.
///
/// This offers efficient serialization of a (Round, Pubkey) tuple.
///
#[repr(transparent)]
#[derive(Serialize, Deserialize, Debug)]
struct KeyAtRound(ArrayVec<u8, 40>);

impl KeyAtRound {
    fn start_of_round(r: Round) -> Self {
        let mut vec: ArrayVec<u8, 40> = ArrayVec::new();
        vec.try_extend_from_slice(&r.to_be_bytes()[..]).unwrap();
        vec.try_extend_from_slice(&[0u8; 32]).unwrap();
        KeyAtRound(vec)
    }

    fn end_of_round(r: Round) -> Self {
        let mut vec: ArrayVec<u8, 40> = ArrayVec::new();
        vec.try_extend_from_slice(&r.to_be_bytes()[..]).unwrap();
        vec.try_extend_from_slice(&[1u8; 32]).unwrap();
        KeyAtRound(vec)
    }

    fn round(&self) -> Round {
        u64::from_be_bytes(self.0[..8].try_into().unwrap())
    }
}

impl From<&(Round, PublicKey)> for KeyAtRound {
    fn from(val: &(Round, PublicKey)) -> Self {
        let mut vec: ArrayVec<u8, 40> = ArrayVec::new();
        vec.try_extend_from_slice(&val.0.to_be_bytes()[..]).unwrap();
        vec.try_extend_from_slice(&val.1 .0[..]).unwrap();
        KeyAtRound(vec)
    }
}

impl From<&KeyAtRound> for (Round, PublicKey) {
    fn from(val: &KeyAtRound) -> Self {
        let r = u64::from_be_bytes(val.0[..8].try_into().unwrap());
        let pub_key = PublicKey(val.0[8..].try_into().unwrap());
        (r, pub_key)
    }
}

/// The representation of the DAG in memory.
type Dag = DBMap<KeyAtRound, (Digest, Certificate)>;

/// The state that needs to be persisted for crash-recovery.
pub struct State {
    /// The last committed round.
    last_committed_round: Round,
    // Keeps the last committed round for each authority. This map is used to clean up the dag and
    // ensure we don't commit twice the same certificate.
    last_committed: HashMap<PublicKey, Round>,
    /// Keeps the latest committed certificate (and its parents) for every authority. Anything older
    /// must be regularly cleaned up through the function `update`.
    dag: Dag,
}

impl State {
    pub fn new<P: AsRef<Path>>(genesis: Vec<Certificate>, db_path: P) -> Result<Self> {
        let genesis = genesis
            .into_iter()
            .map(|x| (x.origin(), (x.digest(), x)))
            .collect::<HashMap<_, _>>();

        let db = Dag::open(db_path, None, None)?;
        db.batch()
            .insert_batch(
                genesis
                    .clone()
                    .into_iter()
                    .map(|(key, val)| (KeyAtRound::from(&(0, key)), val)),
            )?
            .write()?;

        Ok(Self {
            last_committed_round: 0,
            last_committed: genesis.iter().map(|(x, (_, y))| (*x, y.round())).collect(),
            dag: db,
        })
    }

    /// Update and clean up internal state based on committed certificates.
    fn update(&mut self, certificate: &Certificate, gc_depth: Round) -> Result<()> {
        self.last_committed
            .entry(certificate.origin())
            .and_modify(|r| *r = max(*r, certificate.round()))
            .or_insert_with(|| certificate.round());

        let last_committed_round = *self.last_committed.values().max().unwrap();
        self.last_committed_round = last_committed_round;

        // We purge all certificates past the gc bound
        let bound = max(self.last_committed_round, gc_depth + 1);
        self.dag
            .batch()
            .delete_range(
                &KeyAtRound::start_of_round(0),
                &KeyAtRound::start_of_round(bound - gc_depth),
            )?
            .write()?;

        // We purge all certificates for name prior to its last committed round
        let to_purge = self.dag.keys().filter(|kar| {
            let (round, name) = kar.into();
            round < self.last_committed[&name]
        });

        self.dag.batch().delete_batch(to_purge)?.write()
    }
}

pub struct Consensus {
    /// The committee information.
    committee: Committee,
    /// The depth of the garbage collector.
    gc_depth: Round,
    /// The path of the consensus database
    store_path: String,

    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: Receiver<Certificate>,
    /// Outputs the sequence of ordered certificates to the primary (for cleanup and feedback).
    tx_primary: Sender<Certificate>,
    /// Outputs the sequence of ordered certificates to the application layer.
    tx_output: Sender<Certificate>,

    /// The genesis certificates.
    genesis: Vec<Certificate>,
}

impl Consensus {
    pub fn spawn(
        committee: Committee,
        gc_depth: Round,
        store_path: String,
        rx_primary: Receiver<Certificate>,
        tx_primary: Sender<Certificate>,
        tx_output: Sender<Certificate>,
    ) {
        tokio::spawn(async move {
            Self {
                committee: committee.clone(),
                gc_depth,
                store_path,
                rx_primary,
                tx_primary,
                tx_output,
                genesis: Certificate::genesis(&committee),
            }
            .run()
            .await;
        });
    }

    async fn run(&mut self) {
        // The consensus state (everything else is immutable).
        let mut state =
            State::new(self.genesis.clone(), &self.store_path).expect("Database creation failed!");

        // Listen to incoming certificates.
        while let Some(certificate) = self.rx_primary.recv().await {
            let sequence = Consensus::process_certificate(
                &self.committee,
                self.gc_depth,
                &mut state,
                certificate,
            )
            .expect("Certificate processing failed! This may be a lost database connection!");

            // Output the sequence in the right order.
            for certificate in sequence {
                #[cfg(not(feature = "benchmark"))]
                info!("Committed {}", certificate.header);

                #[cfg(feature = "benchmark")]
                for digest in certificate.header.payload.keys() {
                    // NOTE: This log entry is used to compute performance.
                    info!("Committed {} -> {:?}", certificate.header, digest);
                }

                self.tx_primary
                    .send(certificate.clone())
                    .await
                    .expect("Failed to send certificate to primary");

                if let Err(e) = self.tx_output.send(certificate).await {
                    warn!("Failed to output certificate: {}", e);
                }
            }
        }
    }

    pub fn process_certificate(
        committee: &Committee,
        gc_depth: Round,
        state: &mut State,
        certificate: Certificate,
    ) -> Result<Vec<Certificate>> {
        debug!("Processing {:?}", certificate);
        let round = certificate.round();

        // Add the new certificate to the local storage.
        state.dag.insert(
            &KeyAtRound::from(&(round, certificate.origin())),
            &(certificate.digest(), certificate),
        )?;

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
        let (leader_digest, leader) = match Consensus::leader(committee, leader_round, &state.dag) {
            Some(x) => x,
            None => return Ok(Vec::new()),
        };

        // Check if the leader has f+1 support from its children (ie. round r-1).
        let mut stake_iter = state.dag.iter();
        stake_iter.skip_to(&KeyAtRound::start_of_round(r - 1))?;
        let stake: Stake = stake_iter
            .take_while(|(kar, _)| kar.round() == r - 1)
            .flat_map(|(_, (_, x))| {
                if x.header.parents.contains(&leader_digest) {
                    Some(committee.stake(&x.origin()))
                } else {
                    None
                }
            })
            .sum();

        // If it is the case, we can commit the leader. But first, we need to recursively go back to
        // the last committed leader, and commit all preceding leaders in the right order. Committing
        // a leader block means committing all its dependencies.
        if stake < committee.validity_threshold() {
            debug!("Leader {:?} does not have enough support", leader);
            return Ok(Vec::new());
        }

        // Get an ordered list of past leaders that are linked to the current leader.
        debug!("Leader {:?} has enough support", leader);
        let mut sequence = Vec::new();
        for leader in Consensus::order_leaders(committee, &leader, state)?
            .iter()
            .rev()
        {
            // Starting from the oldest leader, flatten the sub-dag referenced by the leader.
            for x in Consensus::order_dag(gc_depth, leader, state)? {
                // Update and clean up internal state.
                state.update(&x, gc_depth)?;

                // Add the certificate to the sequence.
                sequence.push(x);
            }
        }

        // Log the latest committed round of every authority (for debug).
        if log_enabled!(log::Level::Debug) {
            for (name, round) in &state.last_committed {
                debug!("Latest commit of {}: Round {}", name, round);
            }
        }

        Ok(sequence)
    }

    /// Returns the certificate (and the certificate's digest) originated by the leader of the
    /// specified round (if any).
    fn leader(committee: &Committee, round: Round, dag: &Dag) -> Option<(Digest, Certificate)> {
        // TODO: We should elect the leader of round r-2 using the common coin revealed at round r.
        // At this stage, we are guaranteed to have 2f+1 certificates from round r (which is enough to
        // compute the coin). We currently just use round-robin.
        #[cfg(test)]
        let coin = 0;
        #[cfg(not(test))]
        let coin = round;

        // Elect the leader.
        let mut keys: Vec<_> = committee.authorities.keys().cloned().collect();
        keys.sort();
        let leader = keys[coin as usize % committee.size()];

        // Return its certificate and the certificate's digest.
        dag.get(&KeyAtRound::from(&(round, leader)))
            .expect("Leader from known round has no certificate,  argument error")
    }

    /// Order the past leaders that we didn't already commit.
    fn order_leaders(
        committee: &Committee,
        leader: &Certificate,
        state: &State,
    ) -> Result<Vec<Certificate>> {
        let mut to_commit = vec![leader.clone()];
        let mut leader = leader.clone();
        for r in (state.last_committed_round + 2..leader.round())
            .rev()
            .step_by(2)
        {
            // Get the certificate proposed by the previous leader.
            let (_, prev_leader) = match Consensus::leader(committee, r, &state.dag) {
                Some(x) => x,
                None => continue,
            };

            // Check whether there is a path between the last two leaders.
            if Consensus::linked(&leader, &prev_leader, &state.dag)? {
                to_commit.push(prev_leader.clone());
                leader = prev_leader;
            }
        }
        Ok(to_commit)
    }

    /// Checks if there is a path between two leaders.
    fn linked(leader: &Certificate, prev_leader: &Certificate, dag: &Dag) -> Result<bool> {
        let mut parents = vec![leader.clone()];
        let mut siblings: Vec<Certificate> = Vec::new();
        let mut round = leader.round();

        let mut state_iter = dag.iter();
        state_iter.skip_to(&KeyAtRound::end_of_round(leader.round()))?;

        for (kar, (digest, certificate)) in state_iter.rev() {
            if kar.round() < prev_leader.round() {
                break;
            }
            if kar.round() < round {
                parents = siblings;
                siblings = Vec::new();
                round = kar.round();
            }
            if parents.iter().any(|x| x.header.parents.contains(&digest)) {
                siblings.push(certificate)
            }
        }

        Ok(parents.contains(prev_leader))
    }

    /// Flatten the dag referenced by the input certificate. This is a classic depth-first search (pre-order):
    /// https://en.wikipedia.org/wiki/Tree_traversal#Pre-order
    fn order_dag(gc_depth: Round, leader: &Certificate, state: &State) -> Result<Vec<Certificate>> {
        debug!("Processing sub-dag of {:?}", leader);
        let mut ordered = Vec::new();
        let mut already_ordered = HashSet::new();

        let mut buffer = vec![leader.clone()];
        while let Some(x) = buffer.pop() {
            debug!("Sequencing {:?}", x);
            ordered.push(x.clone());

            let mut state_iter = state.dag.iter();
            state_iter.skip_to(&KeyAtRound::start_of_round(x.round() - 1))?;

            let parent_certs = state_iter
                .take_while(|(kar, _)| kar.round() == x.round() - 1)
                .filter(|(_, (d, _))| x.header.parents.contains(d))
                .map(|(_, cert)| cert)
                .collect::<Vec<_>>();

            for (digest, certificate) in parent_certs {
                // We skip the certificate if we (1) already processed it or (2) we reached a round that we already
                // committed for this authority.
                let mut skip = already_ordered.contains(&digest);
                skip |= state
                    .last_committed
                    .get(&certificate.origin())
                    .map_or_else(|| false, |r| r == &certificate.round());
                if !skip {
                    buffer.push(certificate);
                    already_ordered.insert(digest);
                }
            }
        }

        // Ensure we do not commit garbage collected certificates.
        ordered.retain(|x| x.round() + gc_depth >= state.last_committed_round);

        // Ordering the output by round is not really necessary but it makes the commit sequence prettier.
        ordered.sort_by_key(|x| x.round());
        Ok(ordered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    use bincode::Options;
    use primary::Certificate;
    use rand::Rng;

    use super::consensus_tests::*;

    /// KeyAtRound tests
    ///

    #[test]
    fn key_at_round_serialize() {
        let pubkey = keys().get(0).unwrap().0;
        let round: Round = 42;
        let mut target_bytes = [0u8; 40];
        target_bytes[..8].copy_from_slice(&round.to_be_bytes());
        target_bytes[8..].copy_from_slice(&pubkey.0[..]);
        let kar = KeyAtRound::from(&(round, pubkey));
        // reminder: bincode length-prefixes serde's output with a u64 varint by default
        let config = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding();
        assert_eq!(config.serialize(&kar).unwrap()[8..], target_bytes);
    }

    fn temp_dir() -> std::path::PathBuf {
        tempfile::tempdir()
            .expect("Failed to open temporary directory")
            .into_path()
    }

    #[test]
    fn state_limits_test() {
        let nodes = 4; // number of nodes in this test
        let gc_depth = 12;
        let rounds: Round = rand::thread_rng().gen_range(10, 100);

        // process certificates for rounds, check we don't grow the dag too much
        let keys: Vec<_> = keys().into_iter().map(|(x, _)| x).collect();

        let genesis = Certificate::genesis(&mock_committee(&keys[..]))
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        let (certificates, _next_parents) = make_optimal_certificates(1, rounds, &genesis, &keys);
        let committee = mock_committee(&keys);

        let mut state = State::new(Certificate::genesis(&mock_committee(&keys[..])), temp_dir())
            .expect("Failed creating Consensus DB");
        for certificate in certificates {
            Consensus::process_certificate(&committee, gc_depth, &mut state, certificate)
                .expect("Failed processing certificate");
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
        let n = state.dag.keys().count();
        assert!(n <= 6 * nodes, "DAG size: {}", n);
    }

    #[test]
    fn imperfect_state_limits_test() {
        let nodes = 4; // number of nodes in this test
        let gc_depth = 12;
        let rounds: Round = rand::thread_rng().gen_range(10, 100);

        // process certificates for rounds, check we don't grow the dag too much
        let keys: Vec<_> = keys().into_iter().map(|(x, _)| x).collect();

        let genesis = Certificate::genesis(&mock_committee(&keys[..]))
            .iter()
            .map(|x| x.digest())
            .collect::<BTreeSet<_>>();
        // TODO: evidence that this test fails when `failure_probability` parameter >= 1/3
        let (certificates, _next_parents) = make_certificates(1, rounds, &genesis, &keys, 0.333);
        let committee = mock_committee(&keys);

        let mut state = State::new(Certificate::genesis(&mock_committee(&keys[..])), temp_dir())
            .expect("Failed creating consensus DB");
        for certificate in certificates {
            Consensus::process_certificate(&committee, gc_depth, &mut state, certificate)
                .expect("Failed processing certificate!");
        }
        // with "less optimal" certificates (see `make_certificates`), we should keep at most gc_depth rounds lookbehind
        let n = state.dag.keys().count();
        assert!(n <= gc_depth as usize * nodes, "DAG size: {}", n);
    }
}
