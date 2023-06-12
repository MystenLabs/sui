// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::mutable_key_type)]

use crate::bullshark::Bullshark;
use crate::utils::gc_round;
use crate::{metrics::ConsensusMetrics, ConsensusError, SequenceNumber};
use config::{Authority, AuthorityIdentifier, Committee};
use fastcrypto::hash::Hash;
use mysten_metrics::metered_channel;
use mysten_metrics::spawn_logged_monitored_task;
use parking_lot::RwLock;
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::{
    cmp::{max, Ordering},
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};
use storage::{CertificateStore, ConsensusStore};
use tokio::{sync::watch, task::JoinHandle};
use tracing::{debug, info, instrument, trace};
use types::{
    Certificate, CertificateAPI, CertificateDigest, CommittedSubDag, ConditionalBroadcastReceiver,
    ConsensusCommit, HeaderAPI, ReputationScores, Round, Timestamp,
};

#[cfg(test)]
#[path = "tests/consensus_tests.rs"]
pub mod consensus_tests;

/// The representation of the DAG in memory.
pub type Dag = BTreeMap<Round, HashMap<AuthorityIdentifier, (CertificateDigest, Certificate)>>;

#[derive(Default, Clone, Debug)]
pub struct LeaderSwapTable {
    /// The round on which the leader swap table get into effect.
    _round: Round,
    /// The list of `f` (by stake) authorities with best scores as those defined by the provided `ReputationScores`.
    /// Those authorities will be used in the position of the `bad_nodes` on the final leader schedule.
    good_nodes: Vec<Authority>,
    /// The set of `f` (by stake) authorities with the worst scores as those defined by the provided `ReputationScores`.
    /// Every time where such authority is elected as leader on the schedule, it will swapped by one
    /// of the authorities of the `good_nodes`.
    bad_nodes: HashMap<AuthorityIdentifier, Authority>,
}

impl LeaderSwapTable {
    // constructs a new table based on the provided reputation scores.
    pub fn new(committee: &Committee, round: Round, reputation_scores: &ReputationScores) -> Self {
        assert!(reputation_scores.final_of_schedule, "Only reputation scores that have been calculated on the end of a schedule are accepted");

        // calculating the good nodes
        let good_nodes = Self::retrieve_first_nodes(
            committee,
            reputation_scores.authorities_by_score_desc().into_iter(),
        );

        // calculating the bad nodes
        // we revert the sorted authorities to score ascending so we get the first f low scorers
        let bad_nodes = Self::retrieve_first_nodes(
            committee,
            reputation_scores
                .authorities_by_score_desc()
                .into_iter()
                .rev(),
        )
        .into_iter()
        .map(|authority| (authority.id(), authority))
        .collect::<HashMap<AuthorityIdentifier, Authority>>();

        debug!("Reputation scores on round {round}: {reputation_scores:?}");

        Self {
            _round: round,
            good_nodes,
            bad_nodes,
        }
    }

    /// Checks whether the provided leader is a bad performer and needs to be swapped in the schedule
    /// with a good performer. If not, then the method returns None. Otherwise the leader to swap with
    /// is returned instead. The `leader_round` represents the DAG round on which the provided AuthorityIdentifier
    /// is a leader on and is used as a seed to random function in order to calculate the good node that
    /// will swap in that round with the bad node. We are intentionally not doing weighted randomness as
    /// we want to give to all the good nodes equal opportunity to get swapped with bad nodes and not
    /// have one node with enough stake end up swapping bad nodes more frequently than the others on
    /// the final schedule.
    pub fn swap(&self, leader: &AuthorityIdentifier, leader_round: Round) -> Option<Authority> {
        if self.bad_nodes.contains_key(leader) {
            let mut seed_bytes = [0u8; 32];
            seed_bytes[32 - 8..].copy_from_slice(&leader_round.to_le_bytes());
            let mut rng = StdRng::from_seed(seed_bytes);

            let good_node = self
                .good_nodes
                .choose(&mut rng)
                .expect("There should be at least one good node available");

            trace!(
                "Swapping bad leader {} -> {} for round {}",
                leader,
                good_node.id(),
                leader_round
            );

            return Some(good_node.to_owned());
        }
        None
    }

    // Retrieves the first f by stake nodes provided by the iterator `authorities`. It's the
    // caller's responsibility to ensure that the elements of the `authorities` input is already
    // sorted.
    fn retrieve_first_nodes(
        committee: &Committee,
        authorities: impl Iterator<Item = (AuthorityIdentifier, u64)>,
    ) -> Vec<Authority> {
        let mut filtered_authorities = Vec::new();

        let mut stake = 0;
        for (authority_id, _score) in authorities {
            stake += committee.stake_by_id(authority_id);

            // if by adding the authority we have reached validity, then we exit so we make sure that
            // we gather < f + 1
            if committee.reached_validity(stake) {
                break;
            }
            filtered_authorities.push(committee.authority_safe(&authority_id).to_owned());
        }

        filtered_authorities
    }
}

/// The LeaderSchedule is responsible for producing the leader schedule across an epoch. It provides
/// methods to derive the leader of a round based on the provided leader swap table. This struct can
/// be cloned and shared freely as the internal parts are atomically updated.
#[derive(Clone)]
pub struct LeaderSchedule {
    pub committee: Committee,
    pub leader_swap_table: Arc<RwLock<LeaderSwapTable>>,
}

impl LeaderSchedule {
    pub fn new(committee: Committee, table: LeaderSwapTable) -> Self {
        Self {
            committee,
            leader_swap_table: Arc::new(RwLock::new(table)),
        }
    }

    /// Atomically updates the leader swap table with the new provided one. Any leader queried from
    /// now on will get calculated according to this swap table until a new one is provided again.
    pub fn update_leader_swap_table(&self, table: LeaderSwapTable) {
        trace!("Updating swap table {:?}", table);

        let mut write = self.leader_swap_table.write();
        *write = table;
    }

    /// Returns the leader for the provided round. Keep in mind that this method will return a leader
    /// according to the provided LeaderSwapTable. Providing a different table can potentially produce
    /// a different leader for the same round.
    pub fn leader(&self, round: Round) -> Authority {
        assert_eq!(
            round % 2,
            0,
            "We should never attempt to do a leader election for odd rounds"
        );

        // TODO: split the leader election logic for testing from the production code.
        cfg_if::cfg_if! {
        if #[cfg(test)] {
            // We apply round robin in leader election. Since we expect round to be an even number,
            // 2, 4, 6, 8... it can't work well for leader election as we'll omit leaders. Thus
            // we can always divide by 2 to get a monotonically incremented sequence,
            // 2/2 = 1, 4/2 = 2, 6/2 = 3, 8/2 = 4  etc, and then do minus 1 so we can always
            // start with base zero 0.
            let next_leader = (round/2 - 1) as usize % self.committee.size();
            let authorities = self.committee.authorities().collect::<Vec<_>>();

            let leader: Authority = (*authorities.get(next_leader).unwrap()).clone();
            let table = self.leader_swap_table.read();

            table.swap(&leader.id(), round).unwrap_or(leader)
        } else {
            // Elect the leader in a stake-weighted choice seeded by the round
            let leader = self.committee.leader(round);

            let table = self.leader_swap_table.read();
            table.swap(&leader.id(), round).unwrap_or(leader)
        }
        }
    }

    /// Returns the certificate originated by the leader of the specified round (if any). The Authority
    /// leader of the round is always returned and that's irrespective of whether the certificate exists
    /// as that's deterministically determined. The provided `leader_swap_table` is being used to determine
    /// any overrides that need to be performed to the original schedule.
    pub fn leader_certificate<'a>(
        &self,
        round: Round,
        dag: &'a Dag,
    ) -> (Authority, Option<&'a Certificate>) {
        // Note: this function is often called with even rounds only. While we do not aim at random selection
        // yet (see issue https://github.com/MystenLabs/sui/issues/5182), repeated calls to this function
        // should still pick from the whole roster of leaders.
        let leader = self.leader(round);

        // Return its certificate and the certificate's digest.
        match dag.get(&round).and_then(|x| x.get(&leader.id())) {
            None => (leader, None),
            Some((_, certificate)) => (leader, Some(certificate)),
        }
    }
}

/// The state that needs to be persisted for crash-recovery.
pub struct ConsensusState {
    /// The information about the last committed round and corresponding GC round.
    pub last_round: ConsensusRound,
    /// The chosen gc_depth
    pub gc_depth: Round,
    /// Keeps the last committed round for each authority. This map is used to clean up the dag and
    /// ensure we don't commit twice the same certificate.
    pub last_committed: HashMap<AuthorityIdentifier, Round>,
    /// The last committed sub dag. If value is None, it means that we haven't committed any sub dag yet.
    pub last_committed_sub_dag: Option<CommittedSubDag>,
    /// Holds the set of good and bad nodes - as per the last calculated reputation scores - and
    /// performs a necessary swap to ensure that only good leaders will be elected
    pub leader_swap_table: LeaderSwapTable,
    /// Keeps the latest committed certificate (and its parents) for every authority. Anything older
    /// must be regularly cleaned up through the function `update`.
    pub dag: Dag,
    /// Metrics handler
    pub metrics: Arc<ConsensusMetrics>,
}

impl ConsensusState {
    pub fn new(metrics: Arc<ConsensusMetrics>, gc_depth: Round) -> Self {
        Self {
            last_round: ConsensusRound::default(),
            gc_depth,
            last_committed: Default::default(),
            leader_swap_table: Default::default(),
            dag: Default::default(),
            last_committed_sub_dag: None,
            metrics,
        }
    }

    pub fn new_from_store(
        metrics: Arc<ConsensusMetrics>,
        last_committed_round: Round,
        gc_depth: Round,
        recovered_last_committed: HashMap<AuthorityIdentifier, Round>,
        latest_sub_dag: Option<ConsensusCommit>,
        cert_store: CertificateStore,
    ) -> Self {
        let last_round = ConsensusRound::new_with_gc_depth(last_committed_round, gc_depth);

        let dag = Self::construct_dag_from_cert_store(
            &cert_store,
            &recovered_last_committed,
            last_round.gc_round,
        )
        .expect("error when recovering DAG from store");
        metrics.recovered_consensus_state.inc();

        let last_committed_sub_dag = if let Some(latest_sub_dag) = latest_sub_dag.as_ref() {
            let certificates = latest_sub_dag
                .certificates()
                .iter()
                .map(|s| {
                    cert_store
                        .read(*s)
                        .unwrap()
                        .expect("Certificate should be found in database")
                })
                .collect();

            let leader = cert_store
                .read(latest_sub_dag.leader())
                .unwrap()
                .expect("Certificate should be found in database");

            Some(CommittedSubDag::from_commit(
                latest_sub_dag.clone(),
                certificates,
                leader,
            ))
        } else {
            None
        };

        Self {
            gc_depth,
            last_round,
            last_committed: recovered_last_committed,
            last_committed_sub_dag,
            leader_swap_table: Default::default(),
            dag,
            metrics,
        }
    }

    #[instrument(level = "info", skip_all)]
    pub fn construct_dag_from_cert_store(
        cert_store: &CertificateStore,
        last_committed: &HashMap<AuthorityIdentifier, Round>,
        gc_round: Round,
    ) -> Result<Dag, ConsensusError> {
        let mut dag: Dag = BTreeMap::new();

        info!("Recreating dag from last GC round: {}", gc_round);

        // get all certificates at rounds > gc_round
        let certificates = cert_store.after_round(gc_round + 1).unwrap();

        let mut num_certs = 0;
        for cert in &certificates {
            if Self::try_insert_in_dag(&mut dag, last_committed, gc_round, cert)? {
                info!("Inserted certificate: {:?}", cert);
                num_certs += 1;
            }
        }
        info!(
            "Dag is restored and contains {} certs for {} rounds",
            num_certs,
            dag.len()
        );

        Ok(dag)
    }

    /// Returns true if certificate is inserted in the dag.
    pub fn try_insert(&mut self, certificate: &Certificate) -> Result<bool, ConsensusError> {
        Self::try_insert_in_dag(
            &mut self.dag,
            &self.last_committed,
            self.last_round.gc_round,
            certificate,
        )
    }

    /// Returns true if certificate is inserted in the dag.
    fn try_insert_in_dag(
        dag: &mut Dag,
        last_committed: &HashMap<AuthorityIdentifier, Round>,
        gc_round: Round,
        certificate: &Certificate,
    ) -> Result<bool, ConsensusError> {
        if certificate.round() <= gc_round {
            debug!(
                "Ignoring certificate {:?} as it is at or before gc round {}",
                certificate, gc_round
            );
            return Ok(false);
        }
        Self::check_parents(certificate, dag, gc_round);

        // Always insert the certificate even if it is below last committed round of its origin,
        // to allow verifying parent existence.
        if let Some((_, existing_certificate)) = dag.entry(certificate.round()).or_default().insert(
            certificate.origin(),
            (certificate.digest(), certificate.clone()),
        ) {
            // we want to error only if we try to insert a different certificate in the dag
            if existing_certificate.digest() != certificate.digest() {
                return Err(ConsensusError::CertificateEquivocation(
                    certificate.clone(),
                    existing_certificate,
                ));
            }
        }

        Ok(certificate.round()
            > last_committed
                .get(&certificate.origin())
                .cloned()
                .unwrap_or_default())
    }

    /// Update and clean up internal state after committing a certificate.
    pub fn update(&mut self, certificate: &Certificate) {
        self.last_committed
            .entry(certificate.origin())
            .and_modify(|r| *r = max(*r, certificate.round()))
            .or_insert_with(|| certificate.round());
        self.last_round = self.last_round.update(certificate.round(), self.gc_depth);

        self.metrics
            .last_committed_round
            .with_label_values(&[])
            .set(self.last_round.committed_round as i64);
        let elapsed = certificate.metadata().created_at.elapsed().as_secs_f64();
        self.metrics
            .certificate_commit_latency
            .observe(certificate.metadata().created_at.elapsed().as_secs_f64());

        // NOTE: This log entry is used to compute performance.
        tracing::debug!(
            "Certificate {:?} took {} seconds to be committed at round {}",
            certificate.digest(),
            elapsed,
            certificate.round(),
        );

        // Purge all certificates past the gc depth.
        self.dag.retain(|r, _| *r > self.last_round.gc_round);
    }

    // Checks that the provided certificate's parents exist and crashes if not.
    fn check_parents(certificate: &Certificate, dag: &Dag, gc_round: Round) {
        let round = certificate.round();
        // Skip checking parents if they are GC'ed.
        // Also not checking genesis parents for simplicity.
        if round <= gc_round + 1 {
            return;
        }
        if let Some(round_table) = dag.get(&(round - 1)) {
            let store_parents: BTreeSet<&CertificateDigest> =
                round_table.iter().map(|(_, (digest, _))| digest).collect();
            for parent_digest in certificate.header().parents() {
                if !store_parents.contains(parent_digest) {
                    panic!("Parent digest {parent_digest:?} not found in DAG for {certificate:?}!");
                }
            }
        } else {
            panic!("Parent round not found in DAG for {certificate:?}!");
        }
    }

    /// Provides the next index to be used for the next produced sub dag
    pub fn next_sub_dag_index(&self) -> SequenceNumber {
        self.last_committed_sub_dag
            .as_ref()
            .map(|s| s.sub_dag_index)
            .unwrap_or_default()
            + 1
    }
}

/// Holds information about a committed round in consensus. When a certificate gets committed then
/// the corresponding certificate's round is considered a "committed" round. It bears both the
/// committed round and the corresponding garbage collection round.
#[derive(Debug, Default, Copy, Clone)]
pub struct ConsensusRound {
    pub committed_round: Round,
    pub gc_round: Round,
}

impl ConsensusRound {
    pub fn new(committed_round: Round, gc_round: Round) -> Self {
        Self {
            committed_round,
            gc_round,
        }
    }

    pub fn new_with_gc_depth(committed_round: Round, gc_depth: Round) -> Self {
        let gc_round = gc_round(committed_round, gc_depth);

        Self {
            committed_round,
            gc_round,
        }
    }

    /// Calculates the latest CommittedRound by providing a new committed round and the gc_depth.
    /// The method will compare against the existing committed round and return
    /// the updated instance.
    fn update(&self, new_committed_round: Round, gc_depth: Round) -> Self {
        let last_committed_round = max(self.committed_round, new_committed_round);
        let last_gc_round = gc_round(last_committed_round, gc_depth);

        ConsensusRound {
            committed_round: last_committed_round,
            gc_round: last_gc_round,
        }
    }
}

pub struct Consensus {
    /// The committee information.
    committee: Committee,

    /// Receiver for shutdown.
    rx_shutdown: ConditionalBroadcastReceiver,
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_new_certificates: metered_channel::Receiver<Certificate>,
    /// Outputs the sequence of ordered certificates to the primary (for cleanup and feedback).
    tx_committed_certificates: metered_channel::Sender<(Round, Vec<Certificate>)>,
    /// Outputs the highest committed round & corresponding gc_round in the consensus.
    tx_consensus_round_updates: watch::Sender<ConsensusRound>,
    /// Outputs the sequence of ordered certificates to the application layer.
    tx_sequence: metered_channel::Sender<CommittedSubDag>,

    /// The consensus protocol to run.
    protocol: Bullshark,

    /// Metrics handler
    metrics: Arc<ConsensusMetrics>,

    /// Inner state
    state: ConsensusState,
}

impl Consensus {
    #[must_use]
    pub fn spawn(
        committee: Committee,
        gc_depth: Round,
        store: Arc<ConsensusStore>,
        cert_store: CertificateStore,
        rx_shutdown: ConditionalBroadcastReceiver,
        rx_new_certificates: metered_channel::Receiver<Certificate>,
        tx_committed_certificates: metered_channel::Sender<(Round, Vec<Certificate>)>,
        tx_consensus_round_updates: watch::Sender<ConsensusRound>,
        tx_sequence: metered_channel::Sender<CommittedSubDag>,
        protocol: Bullshark,
        metrics: Arc<ConsensusMetrics>,
    ) -> JoinHandle<()> {
        // The consensus state (everything else is immutable).
        let recovered_last_committed = store.read_last_committed();
        let last_committed_round = recovered_last_committed
            .iter()
            .max_by(|a, b| a.1.cmp(b.1))
            .map(|(_k, v)| *v)
            .unwrap_or_else(|| 0);
        let latest_sub_dag = store.get_latest_sub_dag();
        if let Some(sub_dag) = &latest_sub_dag {
            assert_eq!(
                sub_dag.leader_round(),
                last_committed_round,
                "Last subdag leader round {} is not equal to the last committed round {}!",
                sub_dag.leader_round(),
                last_committed_round,
            );
        }

        let state = ConsensusState::new_from_store(
            metrics.clone(),
            last_committed_round,
            gc_depth,
            recovered_last_committed,
            latest_sub_dag,
            cert_store,
        );

        tx_consensus_round_updates
            .send(state.last_round)
            .expect("Failed to send last_committed_round on initialization!");

        let s = Self {
            committee,
            rx_shutdown,
            rx_new_certificates,
            tx_committed_certificates,
            tx_consensus_round_updates,
            tx_sequence,
            protocol,
            metrics,
            state,
        };

        spawn_logged_monitored_task!(s.run(), "Consensus", INFO)
    }

    async fn run(self) {
        match self.run_inner().await {
            Ok(_) => {}
            Err(err @ ConsensusError::ShuttingDown) => {
                debug!("{:?}", err)
            }
            Err(err) => panic!("Failed to run consensus: {:?}", err),
        }
    }

    async fn run_inner(mut self) -> Result<(), ConsensusError> {
        // Listen to incoming certificates.
        'main: loop {
            tokio::select! {

                _ = self.rx_shutdown.receiver.recv() => {
                    return Ok(())
                }

                Some(certificate) = self.rx_new_certificates.recv() => {
                    match certificate.epoch().cmp(&self.committee.epoch()) {
                        Ordering::Equal => {
                            // we can proceed.
                        }
                        _ => {
                            tracing::debug!("Already moved to the next epoch");
                            continue 'main;
                        }
                    }

                    // Process the certificate using the selected consensus protocol.
                    let (_, committed_sub_dags) = self.protocol.process_certificate(&mut self.state, certificate)?;

                    // We extract a list of headers from this specific validator that
                    // have been agreed upon, and signal this back to the narwhal sub-system
                    // to be used to re-send batches that have not made it to a commit.
                    let mut committed_certificates = Vec::new();

                    // Output the sequence in the right order.
                    let mut i = 0;
                    for committed_sub_dag in committed_sub_dags {
                         tracing::debug!("Commit in Sequence {:?}", committed_sub_dag.sub_dag_index);

                        for certificate in &committed_sub_dag.certificates {
                            i+=1;

                            if i % 5_000 == 0 {
                                #[cfg(not(feature = "benchmark"))]
                                tracing::debug!("Committed {}", certificate.header());
                            }

                            #[cfg(feature = "benchmark")]
                            for digest in certificate.header().payload().keys() {
                                // NOTE: This log entry is used to compute performance.
                                tracing::info!("Committed {} -> {:?}", certificate.header(), digest);
                            }

                            committed_certificates.push(certificate.clone());
                        }

                        // NOTE: The size of the sub-dag can be arbitrarily large (depending on the network condition
                        // and Byzantine leaders).
                        self.tx_sequence.send(committed_sub_dag).await.map_err(|_|ConsensusError::ShuttingDown)?;
                    }

                    if !committed_certificates.is_empty(){
                        // Highest committed certificate round is the leader round / commit round
                        // expected by primary.
                        let leader_commit_round = committed_certificates.iter().map(|c| c.round()).max().unwrap();

                        self.tx_committed_certificates
                        .send((leader_commit_round, committed_certificates))
                        .await
                        .map_err(|_|ConsensusError::ShuttingDown)?;

                        assert_eq!(self.state.last_round.committed_round, leader_commit_round);

                        self.tx_consensus_round_updates.send(self.state.last_round)
                        .map_err(|_|ConsensusError::ShuttingDown)?;
                    }

                    self.metrics
                        .consensus_dag_rounds
                        .with_label_values(&[])
                        .set(self.state.dag.len() as i64);
                },

            }
        }
    }
}
