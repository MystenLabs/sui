// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use prometheus::{
    default_registry, register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry, IntCounter, IntGauge, IntGaugeVec, Registry,
};

#[derive(Clone, Debug)]
pub struct ConsensusMetrics {
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
    /// The approximate size in memory (including heap allocations) of the Dag.
    pub dag_size_bytes: IntGauge,
}

impl ConsensusMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            consensus_dag_rounds: register_int_gauge_vec_with_registry!(
                "consensus_dag_rounds",
                "The number of rounds for which the consensus Dag holds certificates",
                &[],
                registry
            ).unwrap(),
            last_committed_round: register_int_gauge_vec_with_registry!(
                "last_committed_round",
                "The most recent round that has been committed from consensus",
                &[],
                registry
            ).unwrap(),
            external_consensus_dag_vertices_elements: register_int_gauge_vec_with_registry!(
                "external_consensus_dag_vertices_elements",
                "The number of elements in the vertices secondary index in the inner dag structure (external consensus)",
                &[],
                registry
            ).unwrap(),
            external_consensus_dag_size: register_int_gauge_vec_with_registry!(
                "external_consensus_dag_size",
                "The number of elements in the inner dag (external consensus)",
                &[],
                registry
            ).unwrap(),
            recovered_consensus_state: register_int_counter_with_registry!(
                "recovered_consensus_state",
                "The number of times the consensus state was restored from the consensus store following a node restart",
                registry
            ).unwrap(),
            recovered_consensus_output: register_int_counter_with_registry!(
                "recovered_consensus_output", 
                "The number of certificates from consensus that were restored and sent to the executor following a node restart",
                registry
            ).unwrap(),
            dag_size_bytes: register_int_gauge_with_registry!(
                "dag_size_bytes",
                "The approximate size in memory (including heap allocations) of the dag",
                registry
            ).unwrap(),
        }
    }
}

impl Default for ConsensusMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}

#[derive(Clone, Debug)]
pub struct ChannelMetrics {
    /// occupancy of the channel from the `Consensus` to `SubscriberHandler`.
    /// See also:
    /// * tx_committed_certificates in primary, where the committed certificates
    /// from `Consensus` are sent to `primary::Core`
    /// * tx_new_certificates where the newly created certificates are sent
    /// from `primary::Core` to `Consensus`
    pub tx_sequence: IntGauge,
}

impl ChannelMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            tx_sequence: register_int_gauge_with_registry!(
                "tx_sequence",
                "occupancy of the channel from the `Consensus` to `SubscriberHandler`",
                registry
            )
            .unwrap(),
        }
    }
}

impl Default for ChannelMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}
