// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const LATENCY_SEC_BUCKETS: &[f64] = &[
    0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 50.0, 100.0, 200.0,
];

#[derive(Clone, Debug)]
pub struct ConsensusMetrics {
    /* TODO: Fix metrics
    /// The number of rounds for which the Dag holds certificates (for Tusk or Bullshark)
    pub consensus_dag_rounds: IntGaugeVec,
    /// The last committed round from consensus
    pub last_committed_round: IntGaugeVec,
    /// The number of elements from the vertices secondary index (external consensus)
    pub external_consensus_dag_vertices_elements: IntGaugeVec,
    /// The number of elements in the dag (external consensus)
    pub external_consensus_dag_size: IntGaugeVec,
    /// The number of times the consensus state was restored from the consensus store
    /// following a node restart
    pub recovered_consensus_state: IntCounter,
    /// The number of certificates from consensus that were restored and sent to the executor
    /// following a node restart
    pub recovered_consensus_output: IntCounter,
    /// The latency between two successful commit rounds
    pub commit_rounds_latency: Histogram,
    /// The number of certificates committed per commit round
    pub committed_certificates: Histogram,
    /// The time it takes for a certificate from the moment it gets created
    /// up to the moment it gets committed.
    pub certificate_commit_latency: Histogram,
    /// When a certificate is received on an odd round, we check
    /// about the previous (even) round leader. We do have three possible cases which
    /// are tagged as values of the label "outcome":
    /// * not_found: the leader certificate has not been found at all
    /// * not_enough_support: when the leader certificate has been found but there was not enough support
    /// * elected: when the leader certificate has been found and had enough support
    pub leader_election: IntCounterVec,
    /// Count leader certificates committed, and whether the leader has strong support.
    pub leader_commits: IntCounterVec,
    */
}

impl ConsensusMetrics {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ConsensusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub struct ChannelMetrics {
    /* TODO: Fix metrics
    /// occupancy of the channel from the `Consensus` to `SubscriberHandler`.
    /// See also:
    /// * tx_committed_certificates in primary, where the committed certificates
    /// from `Consensus` are sent to `primary::StateHandler`
    /// * tx_new_certificates where the newly accepted certificates are sent
    /// from `primary::Synchronizer` to `Consensus`
    pub tx_sequence: IntGauge,
    */
}

impl ChannelMetrics {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ChannelMetrics {
    fn default() -> Self {
        Self::new()
    }
}
