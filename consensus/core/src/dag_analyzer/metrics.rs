// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{fs::File, io::Write, path::Path};

use consensus_config::AuthorityIndex;

pub struct DagAnalysisMetrics {
    pub authority: AuthorityIndex,
    parents_per_authority: Vec<u64>,
    total_blocks: u64,
    total_parents: u64,
    // TODO: Use the block timestamp to deduce the faster proposers.
}

impl DagAnalysisMetrics {
    pub fn new(authority: AuthorityIndex, max_authorities: usize) -> Self {
        Self {
            authority,
            parents_per_authority: vec![0; max_authorities],
            total_blocks: 0,
            total_parents: 0,
        }
    }

    pub fn observe_block(&mut self) {
        self.total_blocks += 1;
    }

    pub fn observe_parent(&mut self, parent: AuthorityIndex) {
        let index = parent.value();
        self.parents_per_authority[index] += 1;
        self.total_parents += 1;
    }

    pub fn average_parents_per_round(&self) -> f64 {
        self.total_parents as f64 / self.total_blocks as f64
    }
}

pub struct MetricsCollection {
    metrics: Vec<DagAnalysisMetrics>,
}

impl MetricsCollection {
    pub fn new(mut metrics: Vec<DagAnalysisMetrics>) -> Self {
        metrics.sort_by_key(|m| m.authority);
        Self { metrics }
    }

    pub fn print_average_parents_per_round(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (x, y): (Vec<_>, Vec<_>) = self
            .metrics
            .iter()
            .map(|metrics| {
                (
                    metrics.authority.value(),
                    metrics.average_parents_per_round(),
                )
            })
            .unzip();

        let filename = "average_parents_per_round.txt";
        self.print_series(x, y, filename)
    }

    pub fn print_peer_connections(&self) -> Result<(), Box<dyn std::error::Error>> {
        let filename = "connections.txt";
        let mut file = File::create(filename)?;

        for metrics in &self.metrics {
            writeln!(
                file,
                "{}",
                metrics
                    .parents_per_authority
                    .iter()
                    .map(|d| {
                        // computer percentage of connections
                        let avg = (*d as f64 / metrics.total_blocks as f64) * 100.0;
                        avg.to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            )?;
        }

        Ok(())
    }

    fn print_series<'a, X, Y, P>(
        &self,
        x: Vec<X>,
        y: Vec<Y>,
        filename: P,
    ) -> Result<(), Box<dyn std::error::Error>>
    where
        X: ToString,
        Y: ToString,
        P: AsRef<Path>,
    {
        let mut file = File::create(filename)?;

        writeln!(
            file,
            "{}",
            x.into_iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )?;
        writeln!(
            file,
            "{}",
            y.into_iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )?;

        Ok(())
    }
}
