// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use prometheus::{
    default_registry, register_int_counter_with_registry, register_int_gauge_vec_with_registry,
    IntCounter, IntGaugeVec, Registry,
};

#[derive(Clone, Debug)]
pub struct ConsensusMetrics {
    /// The number of elements in the Dag (for Tusk or Bullshark)
    pub consensus_dag_size: IntGaugeVec,
    /// The last committed round from consensus
    pub last_committed_round: IntGaugeVec,
    /// The number of elements from the vertices secondary index (external consensus)
    pub external_consensus_dag_vertices_elements: IntGaugeVec,
    /// The number of elements in the dag (external consensus)
    pub external_consensus_dag_size: IntGaugeVec,
    /// The number of times the consensus state was restored from the consensus store
    /// following a node restart
    pub recovered_consensus_state: IntCounter,
}

impl ConsensusMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            consensus_dag_size: register_int_gauge_vec_with_registry!(
                "consensus_dag_size",
                "The number of elements (certificates) in consensus dag",
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
        }
    }
}

impl Default for ConsensusMetrics {
    fn default() -> Self {
        Self::new(default_registry())
    }
}
