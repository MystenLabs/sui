// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use consensus_config::AuthorityIndex;

pub struct DagAnalysisMetrics {
    authority: AuthorityIndex,
    parents_per_authority: HashMap<AuthorityIndex, u64>,
    total_blocks: u64,
    total_parents: u64,
    // TODO: Use the block timestamp to deduce the faster proposers.
}

impl DagAnalysisMetrics {
    pub fn new(authority: AuthorityIndex) -> Self {
        Self {
            authority,
            parents_per_authority: HashMap::new(),
            total_blocks: 0,
            total_parents: 0,
        }
    }

    pub fn observe_block(&mut self) {
        self.total_blocks += 1;
    }

    pub fn observe_parent(&mut self, parent: AuthorityIndex) {
        let count = self.parents_per_authority.entry(parent).or_insert(0);
        *count += 1;
        self.total_parents += 1;
    }

    pub fn print_summary(&self) {
        println!("===============================");
        println!("Authority: {}", self.authority);

        let average_parents_per_round = self.total_parents as f64 / self.total_blocks as f64;
        println!("Average parents per round: {average_parents_per_round}");
        let mut sorted_parents: Vec<_> = self.parents_per_authority.iter().collect();

        sorted_parents.sort_by_key(|(_, count)| *count);
        let best_peers: Vec<_> = sorted_parents
            .iter()
            .map(|(peer, _)| peer)
            .rev()
            .take(10)
            .collect();
        println!("Best connected peers: {best_peers:?}");

        let worst_peers: Vec<_> = sorted_parents
            .iter()
            .map(|(peer, _)| peer)
            .take(10)
            .collect();
        println!("Worst connected peers: {worst_peers:?}");

        println!("===============================");
        println!("");
    }
}
