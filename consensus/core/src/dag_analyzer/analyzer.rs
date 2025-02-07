// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_config::AuthorityIndex;
use futures::future::try_join_all;

use super::metrics::{DagAnalysisMetrics, MetricsCollection};
use crate::{
    block::BlockAPI,
    storage::{rocksdb_store::RocksDBStore, Store},
};

pub async fn read() {
    // The path to the consensus database
    let path = "core/assets/consensus_db/testnet-630";
    // The number of rounds to scan (starting from the last round of the epoch)
    let total_rounds = 10_000;
    // The maximum number of authorities.
    let max_authorities = 115;

    let store = Arc::new(RocksDBStore::new(path));

    let handles: Vec<_> = (0..max_authorities)
        .map(|i| {
            let store = store.clone();
            tokio::spawn(async move {
                let authority = AuthorityIndex::new_for_test(i);
                let mut metrics = DagAnalysisMetrics::new(authority, max_authorities as usize);
                let Ok(blocks) = store.scan_last_blocks_by_author(authority, total_rounds, None)
                else {
                    tracing::warn!("No blocks readable for authority {authority}, skipping");
                    return metrics;
                };

                for block in blocks {
                    metrics.observe_block();
                    let round = block.round();
                    for parent in block.ancestors() {
                        if parent.round == round - 1 {
                            metrics.observe_parent(parent.author);
                        }
                    }
                }

                metrics
            })
        })
        .collect();

    let all_metrics = try_join_all(handles).await.unwrap();
    let collection = MetricsCollection::new(all_metrics);
    collection.print_average_parents_per_round().unwrap();
    collection.print_peer_connections().unwrap();
}
