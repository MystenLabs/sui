// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Confirms the RocksDB column-family stats collector is wired into
//! the service so per-CF sizes and write-stall state are scrapable.
//! The harness registers it the same way `start_service` and
//! `start_restorer` do.

use crate::cluster::LocalCluster;

#[tokio::test]
async fn rocksdb_per_cf_stats_are_exposed() {
    let cluster = LocalCluster::new().await.unwrap();

    let families = cluster.gather_metrics();

    // `total_sst_files_size` is one of the per-CF gauges the collector
    // emits; finding it confirms the collector is registered.
    let sst = families
        .iter()
        .find(|f| f.name().ends_with("total_sst_files_size"))
        .expect("rocksdb total_sst_files_size metric family must be registered");

    // Every CF the collector saw emits its own `cf_name`-labeled
    // series, regardless of whether it currently holds any SSTs.
    let cf_names: Vec<String> = sst
        .get_metric()
        .iter()
        .flat_map(|m| m.get_label())
        .filter(|l| l.name() == "cf_name")
        .map(|l| l.value().to_string())
        .collect();

    for expected in ["objects", "balance", "live_objects", "__watermark"] {
        assert!(
            cf_names.iter().any(|n| n == expected),
            "expected a cf_name={expected} series, saw: {cf_names:?}",
        );
    }

    // The write-stall indicator gauge must also be present so an
    // operator can alert on stalled writes.
    assert!(
        families
            .iter()
            .any(|f| f.name().ends_with("is_write_stopped")),
        "rocksdb is_write_stopped metric family must be registered",
    );
}
